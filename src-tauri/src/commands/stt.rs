use tauri::{AppHandle, Emitter, Manager};
use anyhow::Result;

use crate::state::{AppState, AppStatus};
use crate::audio::AudioCapture;
use crate::platform;

#[tauri::command]
pub fn start_recording(app_handle: AppHandle) -> Result<(), String> {
    do_start_recording(&app_handle).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn stop_recording(app_handle: AppHandle) -> Result<String, String> {
    do_stop_recording(&app_handle).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_status(app_handle: AppHandle) -> Result<String, String> {
    let state = app_handle.state::<AppState>();
    let status = state.status.lock().unwrap().clone();
    serde_json::to_string(&status).map_err(|e| e.to_string())
}

pub fn do_start_recording(app_handle: &AppHandle) -> Result<()> {
    let state = app_handle.state::<AppState>();

    let mut status = state.status.lock().unwrap();
    if *status != AppStatus::Idle {
        anyhow::bail!("Cannot start recording: app is not idle (current: {:?})", *status);
    }

    let mut capture_guard = state.audio_capture.lock().unwrap();
    let mut capture = AudioCapture::new()?;
    capture.start()?;
    *capture_guard = Some(capture);
    *status = AppStatus::Recording;

    let _ = app_handle.emit("recording-status", serde_json::json!({"status": "recording"}));
    let _ = app_handle.emit("overlay-mode", serde_json::json!({"mode": "stt"}));

    tracing::info!("Recording started");
    Ok(())
}

pub async fn do_stop_recording(app_handle: &AppHandle) -> Result<String> {
    let state = app_handle.state::<AppState>();

    let audio_buffer = {
        let mut capture_guard = state.audio_capture.lock().unwrap();
        let capture = capture_guard.as_mut()
            .ok_or_else(|| anyhow::anyhow!("No active recording"))?;
        let buffer = capture.stop()?;
        *capture_guard = None;
        buffer
    };

    {
        let mut status = state.status.lock().unwrap();
        *status = AppStatus::Transcribing;
    }
    let _ = app_handle.emit("recording-status", serde_json::json!({"status": "transcribing"}));

    let result = {
        let engine_guard = state.active_stt_engine.lock().unwrap();
        let engine = engine_guard.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No STT model loaded"))?;
        engine.transcribe(&audio_buffer)?
    };

    tracing::info!("Transcription complete: '{}' ({}ms)", result.text, result.duration_ms);

    let injector = platform::get_text_injector();
    let injection_mode = {
        state.settings.lock().unwrap().stt.injection_mode.clone()
    };

    match injection_mode {
        crate::state::InjectionMode::Keystroke => {
            if injector.is_accessibility_granted() {
                injector.inject_text(&result.text)?;
            } else {
                injector.inject_via_clipboard(&result.text)?;
            }
        }
        crate::state::InjectionMode::Clipboard => {
            injector.inject_via_clipboard(&result.text)?;
        }
    }

    {
        let mut status = state.status.lock().unwrap();
        *status = AppStatus::Idle;
    }

    let _ = app_handle.emit("recording-status", serde_json::json!({"status": "idle"}));
    let _ = app_handle.emit("transcription-complete", serde_json::json!({
        "text": result.text,
        "duration_ms": result.duration_ms,
    }));

    Ok(result.text)
}
