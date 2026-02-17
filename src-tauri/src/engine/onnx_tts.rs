use std::path::Path;
use anyhow::Result;

use super::{Engine, TtsEngine, ModelCapability, ModelInfo, AudioBuffer, TtsOptions};

/// ONNX Runtime TTS engine (Piper, Bark, Coqui XTTS)
/// Full implementation in Phase 6
pub struct OnnxTtsEngine {
    loaded: bool,
}

impl OnnxTtsEngine {
    pub fn new() -> Self {
        Self { loaded: false }
    }
}

impl Engine for OnnxTtsEngine {
    fn load_model(&mut self, _model_path: &Path, _info: &ModelInfo) -> Result<()> {
        todo!("ONNX TTS engine - Phase 6")
    }

    fn unload_model(&mut self) -> Result<()> {
        self.loaded = false;
        Ok(())
    }

    fn is_loaded(&self) -> bool {
        self.loaded
    }

    fn capability(&self) -> ModelCapability {
        ModelCapability::TextToSpeech
    }
}

impl TtsEngine for OnnxTtsEngine {
    fn synthesize(&self, _text: &str, _options: &TtsOptions) -> Result<AudioBuffer> {
        todo!("ONNX TTS synthesis - Phase 6")
    }
}
