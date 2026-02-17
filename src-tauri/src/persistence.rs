use tauri::{AppHandle, Manager};
use tauri_plugin_store::StoreExt;
use crate::state::Settings;

const STORE_FILE: &str = "settings.json";
const SETTINGS_KEY: &str = "settings";

pub fn load_settings(app_handle: &AppHandle) -> Settings {
    let store = match app_handle.store(STORE_FILE) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("Failed to open settings store: {}. Using defaults.", e);
            return Settings::default();
        }
    };

    match store.get(SETTINGS_KEY) {
        Some(value) => {
            match serde_json::from_value::<Settings>(value) {
                Ok(settings) => settings,
                Err(e) => {
                    tracing::warn!("Failed to deserialize stored settings: {}. Using defaults.", e);
                    Settings::default()
                }
            }
        }
        None => {
            tracing::info!("No stored settings found. Using defaults.");
            Settings::default()
        }
    }
}

pub fn save_settings(app_handle: &AppHandle) {
    let state = app_handle.state::<crate::state::AppState>();
    let settings = state.settings.lock().unwrap().clone();

    let store = match app_handle.store(STORE_FILE) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Failed to open settings store for saving: {}", e);
            return;
        }
    };

    match serde_json::to_value(&settings) {
        Ok(value) => {
            store.set(SETTINGS_KEY, value);
            if let Err(e) = store.save() {
                tracing::error!("Failed to save settings store to disk: {}", e);
            }
        }
        Err(e) => {
            tracing::error!("Failed to serialize settings: {}", e);
        }
    }
}
