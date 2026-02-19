use tauri::{AppHandle, Manager};
use crate::state::{AppState, Settings};

#[tauri::command]
pub fn get_settings(app_handle: AppHandle) -> Result<Settings, String> {
    let state = app_handle.state::<AppState>();
    let settings = state.settings.lock().unwrap().clone();
    Ok(settings)
}

#[tauri::command]
pub fn update_settings(app_handle: AppHandle, settings: Settings) -> Result<(), String> {
    let state = app_handle.state::<AppState>();
    let mut current = state.settings.lock().unwrap();

    let stt_active = current.stt.active_model_id.clone();
    let tts_active = current.tts.active_model_id.clone();
    let old_timeout = current.stt.model_idle_timeout_s;

    *current = settings;

    if current.stt.active_model_id.is_none() {
        current.stt.active_model_id = stt_active;
    }
    if current.tts.active_model_id.is_none() {
        current.tts.active_model_id = tts_active;
    }

    let new_timeout = current.stt.model_idle_timeout_s;
    drop(current);

    crate::persistence::save_settings(&app_handle);

    if old_timeout != new_timeout {
        if new_timeout.is_none() {
            crate::commands::stt::cancel_idle_timer(&app_handle);
            let model_id = state.settings.lock().unwrap().stt.active_model_id.clone();
            if let Some(ref mid) = model_id {
                let engine_loaded = state.active_stt_engine.lock().unwrap().is_some();
                if !engine_loaded {
                    if let Err(e) = crate::commands::models::load_stt_engine(&app_handle, mid) {
                        tracing::warn!("Failed to eagerly load engine after disabling idle timeout: {}", e);
                    }
                }
            }
        } else {
            crate::commands::stt::reset_idle_timer(&app_handle);
        }
    }

    Ok(())
}

#[tauri::command]
pub fn update_stt_shortcut(app_handle: AppHandle, shortcut: String) -> Result<(), String> {
    let state = app_handle.state::<AppState>();
    let status = state.status.lock().unwrap().clone();
    if status == crate::state::AppStatus::Recording {
        return Err("Cannot change shortcut while recording".to_string());
    }
    crate::hotkey::update_stt_shortcut(&app_handle, &shortcut).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn check_accessibility_permission() -> Result<bool, String> {
    let injector = crate::platform::get_text_injector();
    Ok(injector.is_accessibility_granted())
}

#[tauri::command]
pub fn request_accessibility_permission() -> Result<(), String> {
    let injector = crate::platform::get_text_injector();
    injector.request_accessibility().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_stt_shortcut_label(app_handle: AppHandle) -> String {
    let state = app_handle.state::<AppState>();
    let shortcut = state.settings.lock().unwrap().shortcuts.stt.clone();
    crate::hotkey::shortcut_display_label(&shortcut).to_string()
}

#[tauri::command]
pub fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
