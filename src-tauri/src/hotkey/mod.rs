use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::ShortcutState;
use anyhow::Result;
use crate::state::RecordingMode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotkeyAction {
    ToggleStt,
    ToggleTts,
}

pub fn handle_hotkey(app_handle: &AppHandle, action: HotkeyAction, shortcut_state: ShortcutState) -> Result<()> {
    match action {
        HotkeyAction::ToggleStt => {
            handle_stt_shortcut(app_handle, shortcut_state)?;
        }
        HotkeyAction::ToggleTts => {
            tracing::warn!("TTS hotkey not yet implemented (Phase 6)");
        }
    }
    Ok(())
}

fn handle_stt_shortcut(app_handle: &AppHandle, shortcut_state: ShortcutState) -> Result<()> {
    let state = app_handle.state::<crate::state::AppState>();
    let recording_mode = state.settings.lock().unwrap().stt.recording_mode.clone();
    let current_status = state.status.lock().unwrap().clone();

    match recording_mode {
        RecordingMode::Toggle => {
            // Only act on key press, ignore release
            if shortcut_state == ShortcutState::Released {
                return Ok(());
            }
            match current_status {
                crate::state::AppStatus::Idle => {
                    crate::commands::stt::do_start_recording(app_handle)?;
                }
                crate::state::AppStatus::Recording => {
                    stop_recording(app_handle);
                }
                _ => {
                    tracing::warn!("Cannot toggle STT in current state: {:?}", current_status);
                }
            }
        }
        RecordingMode::PushToTalk => {
            match shortcut_state {
                ShortcutState::Pressed => {
                    if current_status == crate::state::AppStatus::Idle {
                        crate::commands::stt::do_start_recording(app_handle)?;
                    }
                }
                ShortcutState::Released => {
                    if current_status == crate::state::AppStatus::Recording {
                        stop_recording(app_handle);
                    }
                }
            }
        }
    }

    Ok(())
}

fn stop_recording(app_handle: &AppHandle) {
    let app_handle = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = crate::commands::stt::do_stop_recording(&app_handle).await {
            tracing::error!("Error stopping recording: {}", e);
        }
    });
}
