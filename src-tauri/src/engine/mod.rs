pub mod whisper_stt;
pub mod onnx_stt;
pub mod onnx_tts;

use std::path::Path;
use anyhow::Result;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelCapability {
    SpeechToText,
    TextToSpeech,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineType {
    WhisperCpp,
    Onnx,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub capability: ModelCapability,
    pub engine: EngineType,
    pub languages: Vec<String>,
    pub size_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct AudioBuffer {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionResult {
    pub text: String,
    pub language: Option<String>,
    pub duration_ms: u64,
    pub segments: Option<Vec<Segment>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsOptions {
    pub language: String,
    pub speed: f32,
    pub voice_id: Option<String>,
}

pub trait Engine: Send + Sync {
    fn load_model(&mut self, model_path: &Path, info: &ModelInfo) -> Result<()>;
    fn unload_model(&mut self) -> Result<()>;
    fn is_loaded(&self) -> bool;
    fn capability(&self) -> ModelCapability;
}

pub trait SttEngine: Engine {
    fn transcribe(&self, audio: &AudioBuffer, language: Option<&str>) -> Result<TranscriptionResult>;
    fn warm_up(&self) -> Result<()> { Ok(()) }
    fn cool_down(&self) -> Result<()> { Ok(()) }
}

pub trait TtsEngine: Engine {
    fn synthesize(&self, text: &str, options: &TtsOptions) -> Result<AudioBuffer>;
}

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
