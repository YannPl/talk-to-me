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
    *current = settings;
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
