use std::path::PathBuf;
use std::sync::OnceLock;
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

struct SoundPaths {
    start: PathBuf,
    stop: PathBuf,
}

static SOUND_PATHS: OnceLock<SoundPaths> = OnceLock::new();

fn get_sound_paths() -> &'static SoundPaths {
    SOUND_PATHS.get_or_init(|| {
        let dir = std::env::temp_dir().join("talk-to-me-sounds");
        let _ = std::fs::create_dir_all(&dir);
        let start = dir.join("start.mp3");
        let stop = dir.join("stop.mp3");
        let _ = std::fs::write(&start, include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../src/sounds/start.mp3")));
        let _ = std::fs::write(&stop, include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../src/sounds/stop.mp3")));
        SoundPaths { start, stop }
    })
}

fn play_feedback_sound(app_handle: &AppHandle, sound: &str) {
    let state = app_handle.state::<crate::state::AppState>();
    if !state.settings.lock().unwrap().general.sound_feedback {
        return;
    }
    let paths = get_sound_paths();
    let path = match sound {
        "start" => paths.start.clone(),
        "stop" => paths.stop.clone(),
        _ => return,
    };
    std::thread::spawn(move || {
        let _ = std::process::Command::new("afplay")
            .arg(&path)
            .output();
    });
}

fn handle_stt_shortcut(app_handle: &AppHandle, shortcut_state: ShortcutState) -> Result<()> {
    let state = app_handle.state::<crate::state::AppState>();
    let recording_mode = state.settings.lock().unwrap().stt.recording_mode.clone();
    let current_status = state.status.lock().unwrap().clone();

    match recording_mode {
        RecordingMode::Toggle => {
            if shortcut_state == ShortcutState::Released {
                return Ok(());
            }
            match current_status {
                crate::state::AppStatus::Idle => {
                    play_feedback_sound(app_handle, "start");
                    crate::commands::stt::do_start_recording(app_handle)?;
                }
                crate::state::AppStatus::Recording => {
                    play_feedback_sound(app_handle, "stop");
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
                        play_feedback_sound(app_handle, "start");
                        crate::commands::stt::do_start_recording(app_handle)?;
                    }
                }
                ShortcutState::Released => {
                    if current_status == crate::state::AppStatus::Recording {
                        play_feedback_sound(app_handle, "stop");
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
