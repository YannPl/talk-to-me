use crate::state::RecordingMode;
use anyhow::Result;
use std::path::PathBuf;
use std::sync::OnceLock;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::ShortcutState;

#[cfg(target_os = "macos")]
mod right_cmd;

const VALID_SHORTCUTS: &[&str] = &[
    "Alt+Space",
    "Ctrl+Space",
    "Super+Shift+Space",
    "RightCommand",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotkeyAction {
    ToggleStt,
    ToggleTts,
}

pub fn handle_hotkey(
    app_handle: &AppHandle,
    action: HotkeyAction,
    shortcut_state: ShortcutState,
) -> Result<()> {
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

pub fn shortcut_display_label(shortcut: &str) -> &'static str {
    match shortcut {
        "Alt+Space" => "\u{2325}Space",
        "Ctrl+Space" => "\u{2303}Space",
        "Super+Shift+Space" => "\u{2318}\u{21E7}Space",
        "RightCommand" => "Right \u{2318}",
        _ => "\u{2325}Space",
    }
}

pub fn register_stt_shortcut(app_handle: &AppHandle, shortcut: &str) -> Result<()> {
    if shortcut == "RightCommand" {
        #[cfg(target_os = "macos")]
        {
            right_cmd::start_right_cmd_tap(app_handle)?;
        }
        #[cfg(not(target_os = "macos"))]
        {
            anyhow::bail!("RightCommand shortcut is only supported on macOS");
        }
    } else {
        use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};
        let parsed: Shortcut = shortcut
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid shortcut '{}': {}", shortcut, e))?;
        let app_clone = app_handle.clone();
        app_handle
            .global_shortcut()
            .on_shortcut(parsed, move |_app, _shortcut, event| {
                if let Err(e) = handle_hotkey(&app_clone, HotkeyAction::ToggleStt, event.state) {
                    tracing::error!("Hotkey error: {}", e);
                }
            })?;
    }
    tracing::info!("Registered STT shortcut: {}", shortcut);
    Ok(())
}

pub fn unregister_stt_shortcut(app_handle: &AppHandle, shortcut: &str) -> Result<()> {
    if shortcut == "RightCommand" {
        #[cfg(target_os = "macos")]
        {
            right_cmd::stop_right_cmd_tap();
        }
    } else {
        use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};
        if let Ok(parsed) = shortcut.parse::<Shortcut>() {
            app_handle.global_shortcut().unregister(parsed)?;
        }
    }
    tracing::info!("Unregistered STT shortcut: {}", shortcut);
    Ok(())
}

fn update_tray_shortcut_label(app_handle: &AppHandle, shortcut: &str) {
    let state = app_handle.state::<crate::state::AppState>();
    let guard = state.tray_stt_shortcut_item.lock().unwrap();
    if let Some(ref item) = *guard {
        let label = format!("  Shortcut: {}", shortcut_display_label(shortcut));
        let _ = item.set_text(label);
    }
}

pub fn update_stt_shortcut(app_handle: &AppHandle, new_shortcut: &str) -> Result<()> {
    if !VALID_SHORTCUTS.contains(&new_shortcut) {
        anyhow::bail!("Invalid shortcut: {}", new_shortcut);
    }

    let state = app_handle.state::<crate::state::AppState>();
    let old_shortcut = state.settings.lock().unwrap().shortcuts.stt.clone();

    if old_shortcut == new_shortcut {
        return Ok(());
    }

    // Unregister the old shortcut
    if let Err(e) = unregister_stt_shortcut(app_handle, &old_shortcut) {
        tracing::warn!(
            "Failed to unregister old shortcut '{}': {}",
            old_shortcut,
            e
        );
    }

    // Register the new shortcut
    if let Err(e) = register_stt_shortcut(app_handle, new_shortcut) {
        tracing::error!(
            "Failed to register new shortcut '{}': {}. Rolling back.",
            new_shortcut,
            e
        );
        // Rollback: re-register old shortcut
        let _ = register_stt_shortcut(app_handle, &old_shortcut);
        anyhow::bail!("Failed to register shortcut '{}': {}", new_shortcut, e);
    }

    // Update settings and tray label
    state.settings.lock().unwrap().shortcuts.stt = new_shortcut.to_string();
    crate::persistence::save_settings(app_handle);
    update_tray_shortcut_label(app_handle, new_shortcut);

    let label = shortcut_display_label(new_shortcut);
    let _ = app_handle.emit(
        "stt-shortcut-changed",
        serde_json::json!({ "label": label, "shortcut": new_shortcut }),
    );

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
        let _ = std::fs::write(
            &start,
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../src/sounds/start.mp3"
            )),
        );
        let _ = std::fs::write(
            &stop,
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../src/sounds/stop.mp3"
            )),
        );
        SoundPaths { start, stop }
    })
}

fn pause_system_media() {
    tracing::info!("Checking system media before recording...");
    let mc = crate::platform::get_media_controller();
    mc.pause_if_playing();
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
        let _ = std::process::Command::new("afplay").arg(&path).output();
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
                    pause_system_media();
                    play_feedback_sound(app_handle, "start");
                    crate::commands::stt::do_start_recording(app_handle)?;
                }
                crate::state::AppStatus::Recording | crate::state::AppStatus::Loading => {
                    play_feedback_sound(app_handle, "stop");
                    stop_recording(app_handle);
                }
                _ => {
                    tracing::warn!("Cannot toggle STT in current state: {:?}", current_status);
                }
            }
        }
        RecordingMode::PushToTalk => match shortcut_state {
            ShortcutState::Pressed => {
                if current_status == crate::state::AppStatus::Idle {
                    pause_system_media();
                    play_feedback_sound(app_handle, "start");
                    crate::commands::stt::do_start_recording(app_handle)?;
                }
            }
            ShortcutState::Released => {
                if current_status == crate::state::AppStatus::Recording
                    || current_status == crate::state::AppStatus::Loading
                {
                    play_feedback_sound(app_handle, "stop");
                    stop_recording(app_handle);
                }
            }
        },
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
