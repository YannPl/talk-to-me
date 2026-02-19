use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use serde::{Serialize, Deserialize};

use crate::engine::{SttEngine, TtsEngine, Segment};

pub type CancelFlag = Arc<AtomicBool>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppStatus {
    Idle,
    Loading,
    Recording,
    Transcribing,
    Synthesizing,
    Playing,
}

impl Default for AppStatus {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Debug, Clone, Default)]
pub struct StreamingState {
    pub completed_text: String,
    pub chunks_completed: usize,
    pub locked_language: Option<String>,
    pub total_duration_ms: u64,
    pub segments: Vec<Segment>,
}

pub struct AppState {
    pub active_stt_engine: Mutex<Option<Box<dyn SttEngine>>>,
    pub active_tts_engine: Mutex<Option<Box<dyn TtsEngine>>>,
    pub status: Mutex<AppStatus>,
    pub settings: Mutex<Settings>,
    pub audio_capture: Mutex<Option<crate::audio::AudioCapture>>,
    pub download_cancels: Mutex<HashMap<String, CancelFlag>>,
    pub streaming_state: Mutex<Option<StreamingState>>,
    pub streaming_thread: Mutex<Option<std::thread::JoinHandle<()>>>,
    pub tray_stt_shortcut_item: Mutex<Option<tauri::menu::MenuItem<tauri::Wry>>>,
    pub idle_timer_abort: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            active_stt_engine: Mutex::new(None),
            active_tts_engine: Mutex::new(None),
            status: Mutex::new(AppStatus::default()),
            settings: Mutex::new(Settings::default()),
            audio_capture: Mutex::new(None),
            download_cancels: Mutex::new(HashMap::new()),
            streaming_state: Mutex::new(None),
            streaming_thread: Mutex::new(None),
            tray_stt_shortcut_item: Mutex::new(None),
            idle_timer_abort: Mutex::new(None),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub shortcuts: ShortcutSettings,
    pub stt: SttSettings,
    pub tts: TtsSettings,
    pub general: GeneralSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            shortcuts: ShortcutSettings::default(),
            stt: SttSettings::default(),
            tts: TtsSettings::default(),
            general: GeneralSettings::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShortcutSettings {
    pub stt: String,
    pub tts: String,
}

impl Default for ShortcutSettings {
    fn default() -> Self {
        Self {
            stt: "Alt+Space".to_string(),
            tts: "Alt+Shift+Space".to_string(),
        }
    }
}

fn default_idle_timeout() -> Option<u64> {
    Some(300)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SttSettings {
    pub language: String,
    pub injection_mode: InjectionMode,
    #[serde(default)]
    pub recording_mode: RecordingMode,
    pub active_model_id: Option<String>,
    #[serde(default = "default_idle_timeout")]
    pub model_idle_timeout_s: Option<u64>,
}

impl Default for SttSettings {
    fn default() -> Self {
        Self {
            language: "auto".to_string(),
            injection_mode: InjectionMode::Clipboard,
            recording_mode: RecordingMode::default(),
            active_model_id: None,
            model_idle_timeout_s: Some(300),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InjectionMode {
    Keystroke,
    Clipboard,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordingMode {
    Toggle,
    PushToTalk,
}

impl Default for RecordingMode {
    fn default() -> Self {
        Self::Toggle
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsSettings {
    pub active_model_id: Option<String>,
    pub speed: f32,
    pub voice_id: Option<String>,
}

impl Default for TtsSettings {
    fn default() -> Self {
        Self {
            active_model_id: None,
            speed: 1.0,
            voice_id: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralSettings {
    pub launch_at_login: bool,
    pub sound_feedback: bool,
    #[serde(default)]
    pub onboarding_completed: bool,
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            launch_at_login: false,
            sound_feedback: true,
            onboarding_completed: false,
        }
    }
}
