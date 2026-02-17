pub mod whisper_stt;
pub mod onnx_stt;
pub mod onnx_tts;

use std::path::Path;
use anyhow::Result;
use serde::{Serialize, Deserialize};

/// Model capability direction
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelCapability {
    SpeechToText,
    TextToSpeech,
}

/// Engine runtime type
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineType {
    WhisperCpp,
    Onnx,
}

/// Model metadata, independent of runtime
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub capability: ModelCapability,
    pub engine: EngineType,
    pub languages: Vec<String>,
    pub size_bytes: u64,
}

/// Audio buffer for passing audio data between modules
#[derive(Debug, Clone)]
pub struct AudioBuffer {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
}

/// Transcription result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionResult {
    pub text: String,
    pub language: Option<String>,
    pub duration_ms: u64,
    pub segments: Option<Vec<Segment>>,
}

/// A timed segment of transcription
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

/// TTS synthesis options (future)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsOptions {
    pub language: String,
    pub speed: f32,
    pub voice_id: Option<String>,
}

/// Base engine trait — load/unload models
pub trait Engine: Send + Sync {
    fn load_model(&mut self, model_path: &Path, info: &ModelInfo) -> Result<()>;
    fn unload_model(&mut self) -> Result<()>;
    fn is_loaded(&self) -> bool;
    fn capability(&self) -> ModelCapability;
}

/// STT specialization: audio -> text
pub trait SttEngine: Engine {
    fn transcribe(&self, audio: &AudioBuffer) -> Result<TranscriptionResult>;
}

/// TTS specialization: text -> audio (future)
pub trait TtsEngine: Engine {
    fn synthesize(&self, text: &str, options: &TtsOptions) -> Result<AudioBuffer>;
}

/// Factory to create the right engine based on type and capability
pub fn create_engine(engine_type: &EngineType, capability: &ModelCapability) -> Result<Box<dyn Engine>> {
    match (engine_type, capability) {
        (EngineType::WhisperCpp, ModelCapability::SpeechToText) => {
            Ok(Box::new(whisper_stt::WhisperSttEngine::new()))
        }
        (EngineType::Onnx, ModelCapability::SpeechToText) => {
            Ok(Box::new(onnx_stt::OnnxSttEngine::new()))
        }
        (EngineType::Onnx, ModelCapability::TextToSpeech) => {
            Ok(Box::new(onnx_tts::OnnxTtsEngine::new()))
        }
        _ => anyhow::bail!("Unsupported engine/capability combination: {:?}/{:?}", engine_type, capability),
    }
}
