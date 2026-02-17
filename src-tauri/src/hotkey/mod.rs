use tauri::{AppHandle, Manager};
use anyhow::Result;

/// Action triggered by a global shortcut
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotkeyAction {
    ToggleStt,
    ToggleTts, // Future
}

/// Handle a hotkey action by dispatching to the appropriate pipeline
pub fn handle_hotkey(app_handle: &AppHandle, action: HotkeyAction) -> Result<()> {
    match action {
        HotkeyAction::ToggleStt => {
            handle_stt_toggle(app_handle)?;
        }
        HotkeyAction::ToggleTts => {
            tracing::warn!("TTS hotkey not yet implemented (Phase 6)");
        }
    }
    Ok(())
}

fn handle_stt_toggle(app_handle: &AppHandle) -> Result<()> {
    let state = app_handle.state::<crate::state::AppState>();
    let current_status = state.status.lock().unwrap().clone();

    match current_status {
        crate::state::AppStatus::Idle => {
            // Start recording
            crate::commands::stt::do_start_recording(app_handle)?;
        }
        crate::state::AppStatus::Recording => {
            // Stop recording and transcribe
            let app_handle = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = crate::commands::stt::do_stop_recording(&app_handle).await {
                    tracing::error!("Error stopping recording: {}", e);
                }
            });
        }
        _ => {
            tracing::warn!("Cannot toggle STT in current state: {:?}", current_status);
        }
    }

    Ok(())
}
