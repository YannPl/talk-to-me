use std::path::Path;
use std::sync::Mutex;
use anyhow::{Result, Context};
use whisper_rs::{WhisperContext, WhisperContextParameters, FullParams, SamplingStrategy};

use super::{Engine, SttEngine, ModelCapability, ModelInfo, AudioBuffer, TranscriptionResult, Segment};

pub struct WhisperSttEngine {
    context: Mutex<Option<WhisperContext>>,
}

impl WhisperSttEngine {
    pub fn new() -> Self {
        Self {
            context: Mutex::new(None),
        }
    }
}

impl Engine for WhisperSttEngine {
    fn load_model(&mut self, model_path: &Path, _info: &ModelInfo) -> Result<()> {
        let params = WhisperContextParameters::default();
        let ctx = WhisperContext::new_with_params(
            model_path.to_str().context("Invalid model path")?,
            params,
        ).map_err(|e| anyhow::anyhow!("Failed to load Whisper model: {}", e))?;

        *self.context.lock().unwrap() = Some(ctx);
        Ok(())
    }

    fn unload_model(&mut self) -> Result<()> {
        *self.context.lock().unwrap() = None;
        Ok(())
    }

    fn is_loaded(&self) -> bool {
        self.context.lock().unwrap().is_some()
    }

    fn capability(&self) -> ModelCapability {
        ModelCapability::SpeechToText
    }
}

impl SttEngine for WhisperSttEngine {
    fn transcribe(&self, audio: &AudioBuffer) -> Result<TranscriptionResult> {
        let ctx_guard = self.context.lock().unwrap();
        let ctx = ctx_guard.as_ref().context("Model not loaded")?;

        let mut state = ctx.create_state().map_err(|e| anyhow::anyhow!("Failed to create state: {}", e))?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_language(Some("auto"));

        let start = std::time::Instant::now();

        state.full(params, &audio.samples)
            .map_err(|e| anyhow::anyhow!("Transcription failed: {}", e))?;

        let duration_ms = start.elapsed().as_millis() as u64;

        let num_segments = state.full_n_segments()
            .map_err(|e| anyhow::anyhow!("Failed to get segments: {}", e))?;

        let mut text = String::new();
        let mut segments = Vec::new();

        for i in 0..num_segments {
            let segment_text = state.full_get_segment_text(i)
                .map_err(|e| anyhow::anyhow!("Failed to get segment text: {}", e))?;
            let start_ts = state.full_get_segment_t0(i)
                .map_err(|e| anyhow::anyhow!("Failed to get segment start: {}", e))?;
            let end_ts = state.full_get_segment_t1(i)
                .map_err(|e| anyhow::anyhow!("Failed to get segment end: {}", e))?;

            text.push_str(&segment_text);
            segments.push(Segment {
                start_ms: (start_ts * 10) as u64,
                end_ms: (end_ts * 10) as u64,
                text: segment_text,
            });
        }

        let language = state.full_lang_id_from_state()
            .ok()
            .and_then(|id| {
                whisper_rs::get_lang_str(id).map(|s| s.to_string())
            });

        Ok(TranscriptionResult {
            text: text.trim().to_string(),
            language,
            duration_ms,
            segments: Some(segments),
        })
    }
}
