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

    // Preserve active_model_id fields — these are managed by set_active_model/delete_model,
    // not by the frontend settings form (which sends null)
    let stt_active = current.stt.active_model_id.clone();
    let tts_active = current.tts.active_model_id.clone();

    *current = settings;

    if current.stt.active_model_id.is_none() {
        current.stt.active_model_id = stt_active;
    }
    if current.tts.active_model_id.is_none() {
        current.tts.active_model_id = tts_active;
    }

    drop(current); // release lock before save_settings re-acquires it
    crate::persistence::save_settings(&app_handle);
    Ok(())
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
pub fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
