use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use anyhow::Result;

use crate::state::{AppState, AppStatus, StreamingState};
use crate::audio::AudioCapture;
use crate::audio::processing::{split_at_silence, resample};
use crate::engine::{AudioBuffer, TranscriptionResult};
use crate::platform;

const STREAMING_CHUNK_DURATION_S: f32 = 20.0;
const STREAMING_SEARCH_WINDOW_S: f32 = 2.0;
const STREAMING_RMS_WINDOW_MS: f32 = 100.0;
const STREAMING_POLL_INTERVAL_MS: u64 = 500;
const TARGET_SAMPLE_RATE: u32 = 16000;

/// Joins chunk text to accumulated text, smoothing artificial punctuation at boundaries.
/// When the previous chunk ends with a sentence-ending punct (`.!?`) and the new chunk
/// starts with a lowercase letter, the punct was likely added by the model because it saw
/// the end of the audio segment — not a real sentence boundary. We remove it.
fn append_chunk_text(accumulated: &mut String, new_chunk: &str) {
    let new_chunk = new_chunk.trim();
    if new_chunk.is_empty() {
        return;
    }
    if accumulated.is_empty() {
        accumulated.push_str(new_chunk);
        return;
    }

    let next_starts_lowercase = new_chunk.chars().next().map_or(false, |c| c.is_lowercase());
    let prev_ends_with_sentence_punct = accumulated.ends_with('.')
        || accumulated.ends_with('!')
        || accumulated.ends_with('?');

    if prev_ends_with_sentence_punct && next_starts_lowercase {
        accumulated.pop();
    }

    accumulated.push(' ');
    accumulated.push_str(new_chunk);
}

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

    let (monitor, drain) = {
        let mut status = state.status.lock().unwrap();
        if *status != AppStatus::Idle {
            anyhow::bail!("Cannot start recording: app is not idle (current: {:?})", *status);
        }

        let mut capture_guard = state.audio_capture.lock().unwrap();
        let mut capture = AudioCapture::new()?;
        capture.start()?;
        let monitor = capture.level_monitor();
        let drain = capture.streaming_drain();
        *capture_guard = Some(capture);
        *status = AppStatus::Recording;
        (monitor, drain)
    };

    {
        let mut streaming = state.streaming_state.lock().unwrap();
        *streaming = Some(StreamingState::default());
    }

    // Audio level monitor thread
    let handle = app_handle.clone();
    std::thread::spawn(move || {
        while monitor.is_active() {
            let level = (monitor.current_level() * 8.0).sqrt().min(1.0);
            let _ = handle.emit("audio-level", serde_json::json!({"level": level}));
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    });

    // Streaming transcription thread
    let handle_streaming = app_handle.clone();
    let streaming_handle = std::thread::spawn(move || {
        streaming_transcription_loop(handle_streaming, drain);
    });
    {
        let mut thread_guard = state.streaming_thread.lock().unwrap();
        *thread_guard = Some(streaming_handle);
    }

    if let Some(window) = app_handle.get_webview_window("overlay") {
        let _ = window.show();
    }

    let _ = app_handle.emit("recording-status", serde_json::json!({"status": "recording"}));
    let _ = app_handle.emit("overlay-mode", serde_json::json!({"mode": "stt"}));

    let handle_for_shortcut = app_handle.clone();
    std::thread::spawn(move || {
        register_cancel_shortcut(&handle_for_shortcut);
    });

    tracing::info!("Recording started");
    Ok(())
}

fn streaming_transcription_loop(
    app_handle: AppHandle,
    drain: crate::audio::capture::StreamingDrain,
) {
    {
        let state = app_handle.state::<AppState>();
        let engine_guard = state.active_stt_engine.lock().unwrap();
        if let Some(engine) = engine_guard.as_ref() {
            if let Err(e) = engine.warm_up() {
                tracing::error!("Engine warm_up failed: {}", e);
            }
        }
    }

    let device_rate = drain.device_sample_rate();
    let threshold = (STREAMING_CHUNK_DURATION_S * device_rate as f32) as usize;

    loop {
        std::thread::sleep(std::time::Duration::from_millis(STREAMING_POLL_INTERVAL_MS));

        if !drain.is_active() {
            break;
        }

        if drain.available_samples() < threshold {
            continue;
        }

        let raw = drain.drain();
        if raw.is_empty() {
            continue;
        }

        let resampled = match resample(&raw, device_rate, TARGET_SAMPLE_RATE) {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("Streaming resample error: {}", e);
                continue;
            }
        };

        let chunks = split_at_silence(
            &resampled,
            TARGET_SAMPLE_RATE,
            STREAMING_CHUNK_DURATION_S,
            STREAMING_SEARCH_WINDOW_S,
            STREAMING_RMS_WINDOW_MS,
        );

        let state = app_handle.state::<AppState>();

        for chunk in &chunks {
            if !drain.is_active() {
                break;
            }

            let chunk_samples = &resampled[chunk.start_sample..chunk.end_sample];
            let chunk_audio = AudioBuffer {
                samples: chunk_samples.to_vec(),
                sample_rate: TARGET_SAMPLE_RATE,
                channels: 1,
            };

            {
                let streaming = state.streaming_state.lock().unwrap();
                if streaming.is_none() {
                    break; // cancelled
                }
            }

            let language = {
                let settings = state.settings.lock().unwrap();
                let lang = settings.stt.language.clone();
                if lang == "auto" { None } else { Some(lang) }
            };

            let chunk_result = {
                let engine_guard = state.active_stt_engine.lock().unwrap();
                let engine = match engine_guard.as_ref() {
                    Some(e) => e,
                    None => {
                        tracing::error!("Streaming: no STT engine loaded");
                        break;
                    }
                };
                match engine.transcribe(&chunk_audio, language.as_deref()) {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::error!("Streaming transcription error: {}", e);
                        continue;
                    }
                }
            };

            tracing::info!(
                "Streaming chunk: '{}' ({}ms, lang={:?})",
                chunk_result.text, chunk_result.duration_ms, chunk_result.language
            );

            {
                let mut streaming = state.streaming_state.lock().unwrap();
                if let Some(ref mut s) = *streaming {
                    if s.locked_language.is_none() {
                        if let Some(ref detected) = chunk_result.language {
                            tracing::info!("Streaming: detected language '{}'", detected);
                            s.locked_language = Some(detected.clone());
                        }
                    }

                    append_chunk_text(&mut s.completed_text, &chunk_result.text);

                    if let Some(segs) = chunk_result.segments {
                        s.segments.extend(segs);
                    }

                    s.total_duration_ms += chunk_result.duration_ms;
                    s.chunks_completed += 1;

                    let _ = app_handle.emit("streaming-transcription", serde_json::json!({
                        "chunks_completed": s.chunks_completed,
                        "text": s.completed_text,
                    }));
                }
            }
        }
    }

    tracing::info!("Streaming transcription loop exited");
}

pub async fn do_stop_recording(app_handle: &AppHandle) -> Result<String> {
    let handle_for_shortcut = app_handle.clone();
    std::thread::spawn(move || {
        unregister_cancel_shortcut(&handle_for_shortcut);
    });
    let state = app_handle.state::<AppState>();

    // Stop capture — sets is_recording=false, returns only samples accumulated since last drain
    let tail_raw = {
        let mut capture_guard = state.audio_capture.lock().unwrap();
        let capture = capture_guard.as_mut()
            .ok_or_else(|| anyhow::anyhow!("No active recording"))?;
        let buffer = capture.stop()?;
        *capture_guard = None;
        buffer
    };

    // Wait for streaming thread to finish its in-flight chunk before taking results
    {
        let thread_handle = state.streaming_thread.lock().unwrap().take();
        if let Some(handle) = thread_handle {
            let _ = handle.join();
        }
    }

    // Now safe to take streaming results — all chunks are written
    let streaming = {
        let mut streaming_guard = state.streaming_state.lock().unwrap();
        streaming_guard.take().unwrap_or_default()
    };

    {
        let mut status = state.status.lock().unwrap();
        *status = AppStatus::Transcribing;
    }
    let _ = app_handle.emit("recording-status", serde_json::json!({"status": "transcribing"}));

    let mut full_text = streaming.completed_text;
    let mut all_segments = streaming.segments;
    let mut total_duration_ms = streaming.total_duration_ms;
    let mut result_language = streaming.locked_language.clone();

    let language = {
        let settings = state.settings.lock().unwrap();
        let lang = settings.stt.language.clone();
        if lang == "auto" { None } else { Some(lang) }
    };

    // Transcribe the tail (samples since last drain — already resampled by capture.stop())
    let tail_samples = tail_raw.samples;

    if !tail_samples.is_empty() {
        let tail_chunks = split_at_silence(
            &tail_samples,
            TARGET_SAMPLE_RATE,
            STREAMING_CHUNK_DURATION_S,
            STREAMING_SEARCH_WINDOW_S,
            STREAMING_RMS_WINDOW_MS,
        );

        tracing::info!(
            "Tail audio: {} samples, {} chunk(s) (streaming had {} chunks)",
            tail_samples.len(), tail_chunks.len(), streaming.chunks_completed
        );

        for chunk in &tail_chunks {
            let chunk_audio = AudioBuffer {
                samples: tail_samples[chunk.start_sample..chunk.end_sample].to_vec(),
                sample_rate: TARGET_SAMPLE_RATE,
                channels: 1,
            };

            let chunk_result = {
                let engine_guard = state.active_stt_engine.lock().unwrap();
                let engine = engine_guard.as_ref()
                    .ok_or_else(|| anyhow::anyhow!("No STT model loaded"))?;
                engine.transcribe(&chunk_audio, language.as_deref())?
            };

            tracing::info!("Tail chunk: '{}' ({}ms)", chunk_result.text, chunk_result.duration_ms);

            append_chunk_text(&mut full_text, &chunk_result.text);

            if let Some(segs) = chunk_result.segments {
                all_segments.extend(segs);
            }

            total_duration_ms += chunk_result.duration_ms;
            if result_language.is_none() {
                result_language = chunk_result.language;
            }
        }
    } else {
        tracing::info!("No tail audio (streaming had {} chunks)", streaming.chunks_completed);
    }

    {
        let engine_guard = state.active_stt_engine.lock().unwrap();
        if let Some(engine) = engine_guard.as_ref() {
            if let Err(e) = engine.cool_down() {
                tracing::error!("Engine cool_down failed: {}", e);
            }
        }
    }

    let result = TranscriptionResult {
        text: full_text,
        language: result_language,
        duration_ms: total_duration_ms,
        segments: if all_segments.is_empty() { None } else { Some(all_segments) },
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

    let handle_for_hide = app_handle.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(500));
        let state = handle_for_hide.state::<AppState>();
        if *state.status.lock().unwrap() == AppStatus::Idle {
            if let Some(window) = handle_for_hide.get_webview_window("overlay") {
                let _ = window.hide();
            }
        }
    });

    Ok(result.text)
}

pub fn do_cancel_recording(app_handle: &AppHandle) -> Result<()> {
    let state = app_handle.state::<AppState>();

    {
        let mut capture_guard = state.audio_capture.lock().unwrap();
        if let Some(capture) = capture_guard.as_mut() {
            let _ = capture.stop();
        }
        *capture_guard = None;
    }

    {
        let mut streaming = state.streaming_state.lock().unwrap();
        *streaming = None;
    }

    // Drop the handle — streaming thread will exit on its own (is_active=false)
    {
        let _ = state.streaming_thread.lock().unwrap().take();
    }

    {
        let mut status = state.status.lock().unwrap();
        *status = AppStatus::Idle;
    }

    {
        let engine_guard = state.active_stt_engine.lock().unwrap();
        if let Some(engine) = engine_guard.as_ref() {
            if let Err(e) = engine.cool_down() {
                tracing::error!("Engine cool_down failed: {}", e);
            }
        }
    }

    let _ = app_handle.emit("recording-status", serde_json::json!({"status": "idle"}));

    if let Some(window) = app_handle.get_webview_window("overlay") {
        let _ = window.hide();
    }

    let handle = app_handle.clone();
    std::thread::spawn(move || {
        unregister_cancel_shortcut(&handle);
    });

    tracing::info!("Recording cancelled");
    Ok(())
}

fn register_cancel_shortcut(app_handle: &AppHandle) {
    let escape: Shortcut = "Escape".parse().unwrap();
    let handle = app_handle.clone();
    if let Err(e) = app_handle.global_shortcut().on_shortcut(escape, move |_app, _shortcut, event| {
        if event.state == ShortcutState::Pressed {
            if let Err(e) = do_cancel_recording(&handle) {
                tracing::error!("Cancel recording error: {}", e);
            }
        }
    }) {
        tracing::error!("Failed to register Escape shortcut: {}", e);
    }
}

fn unregister_cancel_shortcut(app_handle: &AppHandle) {
    let escape: Shortcut = "Escape".parse().unwrap();
    let _ = app_handle.global_shortcut().unregister(escape);
}
