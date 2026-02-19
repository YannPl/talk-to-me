use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Manager};
use crate::hub::registry::{self, CatalogModel, InstalledModel};
use crate::engine::{Engine, ModelCapability};

#[tauri::command]
pub fn list_installed_models(capability: Option<String>) -> Result<Vec<InstalledModel>, String> {
    let cap_filter = capability.as_deref().map(|c| match c {
        "stt" => ModelCapability::SpeechToText,
        "tts" => ModelCapability::TextToSpeech,
        _ => ModelCapability::SpeechToText,
    });
    registry::list_installed_models(cap_filter.as_ref()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_catalog(capability: Option<String>) -> Result<Vec<CatalogModel>, String> {
    let mut catalog = registry::load_catalog().map_err(|e| e.to_string())?;

    if let Some(cap_str) = capability {
        let cap = match cap_str.as_str() {
            "stt" => ModelCapability::SpeechToText,
            "tts" => ModelCapability::TextToSpeech,
            _ => return Err("Invalid capability".into()),
        };
        catalog.retain(|m| m.capability == cap);
    }

    Ok(catalog)
}

#[tauri::command]
pub async fn download_model(app_handle: AppHandle, model_id: String) -> Result<(), String> {
    let catalog = registry::load_catalog().map_err(|e| e.to_string())?;
    let model = catalog.iter()
        .find(|m| m.id == model_id)
        .ok_or_else(|| format!("Model not found in catalog: {}", model_id))?
        .clone();

    let models_dir = registry::models_dir().map_err(|e| e.to_string())?;
    let cap_dir = match model.capability {
        ModelCapability::SpeechToText => "stt",
        ModelCapability::TextToSpeech => "tts",
    };
    let model_slug = model_id.replace('/', "--");
    let model_dir = models_dir.join(cap_dir).join(&model_slug);

    let total_size: u64 = model.files.iter().map(|f| f.size_bytes).sum();

    let cancel_flag = Arc::new(AtomicBool::new(false));
    {
        let state = app_handle.state::<crate::state::AppState>();
        state.download_cancels.lock().unwrap()
            .insert(model_id.clone(), cancel_flag.clone());
    }

    let result = async {
        for file in &model.files {
            let hf_repo = file.hf_repo.as_deref().unwrap_or(&model_id);
            let url = crate::hub::api::download_url(hf_repo, &file.filename);
            let local_name = file.local_filename.as_deref().unwrap_or(&file.filename);
            let dest = model_dir.join(local_name);

            crate::hub::download::download_file(
                &app_handle,
                &model_id,
                &url,
                &dest,
                file.size_bytes,
                &cancel_flag,
            ).await.map_err(|e| e.to_string())?;
        }
        Ok::<(), String>(())
    }.await;

    {
        let state = app_handle.state::<crate::state::AppState>();
        state.download_cancels.lock().unwrap().remove(&model_id);
    }

    if let Err(ref e) = result {
        if e.contains("cancelled") {
            tracing::info!("Cleaning up cancelled download: {}", model_id);
            let _ = std::fs::remove_dir_all(&model_dir);
            return Err("cancelled".to_string());
        }
    }

    result?;

    let installed = InstalledModel {
        id: model_id.clone(),
        name: model.name.clone(),
        capability: model.capability.clone(),
        engine: model.engine.clone(),
        path: model_dir.to_string_lossy().to_string(),
        installed_at: chrono_now(),
        size_bytes: total_size,
    };
    registry::add_installed_model(&installed).map_err(|e| e.to_string())?;

    tracing::info!("Model installed: {} at {}", model_id, model_dir.display());

    if model.capability == ModelCapability::SpeechToText {
        let state = app_handle.state::<crate::state::AppState>();
        let current_active = state.settings.lock().unwrap().stt.active_model_id.clone();
        if current_active.is_none() {
            load_stt_engine(&app_handle, &model_id).map_err(|e| e.to_string())?;
            crate::persistence::save_settings(&app_handle);
        }
    }

    Ok(())
}

#[tauri::command]
pub fn cancel_download(app_handle: AppHandle, model_id: String) -> Result<(), String> {
    let state = app_handle.state::<crate::state::AppState>();
    let cancels = state.download_cancels.lock().unwrap();
    if let Some(flag) = cancels.get(&model_id) {
        (**flag).store(true, Ordering::Relaxed);
        tracing::info!("Cancellation requested for: {}", model_id);
    }
    Ok(())
}

#[tauri::command]
pub fn delete_model(app_handle: AppHandle, model_id: String) -> Result<(), String> {
    let mut settings_changed = false;
    {
        let state = app_handle.state::<crate::state::AppState>();
        let mut settings = state.settings.lock().unwrap();
        if settings.stt.active_model_id.as_deref() == Some(&model_id) {
            *state.active_stt_engine.lock().unwrap() = None;
            settings.stt.active_model_id = None;
            settings_changed = true;
            tracing::info!("Unloaded active STT engine before deleting model: {}", model_id);
        }
        if settings.tts.active_model_id.as_deref() == Some(&model_id) {
            *state.active_tts_engine.lock().unwrap() = None;
            settings.tts.active_model_id = None;
            settings_changed = true;
        }
    }

    let models_dir = registry::models_dir().map_err(|e| e.to_string())?;
    let model_slug = model_id.replace('/', "--");

    for cap_dir in &["stt", "tts"] {
        let path = models_dir.join(cap_dir).join(&model_slug);
        if path.exists() {
            std::fs::remove_dir_all(&path).map_err(|e| e.to_string())?;
            tracing::info!("Deleted model directory: {}", path.display());
        }
    }

    registry::remove_installed_model(&model_id).map_err(|e| e.to_string())?;

    if settings_changed {
        crate::persistence::save_settings(&app_handle);
    }

    Ok(())
}

#[tauri::command]
pub fn set_active_model(app_handle: AppHandle, model_id: String, capability: String) -> Result<(), String> {
    match capability.as_str() {
        "stt" => {
            load_stt_engine(&app_handle, &model_id).map_err(|e| e.to_string())?;
            crate::commands::stt::reset_idle_timer(&app_handle);
        }
        "tts" => {
            let state = app_handle.state::<crate::state::AppState>();
            state.settings.lock().unwrap().tts.active_model_id = Some(model_id);
        }
        _ => return Err("Invalid capability".into()),
    }
    crate::persistence::save_settings(&app_handle);

    Ok(())
}

#[tauri::command]
pub fn get_active_model(app_handle: AppHandle, capability: String) -> Result<Option<String>, String> {
    let state = app_handle.state::<crate::state::AppState>();
    let settings = state.settings.lock().unwrap();

    Ok(match capability.as_str() {
        "stt" => settings.stt.active_model_id.clone(),
        "tts" => settings.tts.active_model_id.clone(),
        _ => None,
    })
}

pub(crate) fn load_stt_engine(app_handle: &AppHandle, model_id: &str) -> anyhow::Result<()> {
    use crate::engine::{self, ModelInfo, EngineType, SttEngine};

    let installed = registry::list_installed_models(Some(&ModelCapability::SpeechToText))?;
    let model = installed.iter()
        .find(|m| m.id == model_id)
        .ok_or_else(|| anyhow::anyhow!("Model not installed: {}", model_id))?;

    let model_dir = std::path::PathBuf::from(&model.path);

    let info = ModelInfo {
        id: model.id.clone(),
        name: model.name.clone(),
        capability: ModelCapability::SpeechToText,
        engine: model.engine.clone(),
        languages: vec![],
        size_bytes: model.size_bytes,
    };

    let engine: Box<dyn SttEngine> = match model.engine {
        EngineType::WhisperCpp => {
            let model_file = std::fs::read_dir(&model_dir)?
                .filter_map(|e| e.ok())
                .find(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    name.ends_with(".bin")
                })
                .ok_or_else(|| anyhow::anyhow!("No .bin model file found in {}", model_dir.display()))?;

            let model_path = model_file.path();
            let mut eng = engine::whisper_stt::WhisperSttEngine::new();
            eng.load_model(&model_path, &info)?;
            tracing::info!("WhisperCpp STT engine loaded: {} from {}", model_id, model_path.display());
            Box::new(eng)
        }
        EngineType::Onnx => {
            let mut eng = engine::onnx_stt::OnnxSttEngine::new();
            eng.load_model(&model_dir, &info)?;
            tracing::info!("ONNX STT engine loaded: {} from {}", model_id, model_dir.display());
            Box::new(eng)
        }
    };

    let state = app_handle.state::<crate::state::AppState>();
    *state.active_stt_engine.lock().unwrap() = Some(engine);
    state.settings.lock().unwrap().stt.active_model_id = Some(model_id.to_string());

    Ok(())
}

fn chrono_now() -> String {
    let dur = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", dur.as_secs())
}
