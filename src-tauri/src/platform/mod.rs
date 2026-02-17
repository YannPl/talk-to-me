#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;

// Future: windows.rs, linux.rs

use anyhow::Result;

/// Inject text into the active application (STT -> app)
pub trait TextInjector: Send + Sync {
    fn inject_text(&self, text: &str) -> Result<()>;
    fn inject_via_clipboard(&self, text: &str) -> Result<()>;
    fn is_accessibility_granted(&self) -> bool;
    fn request_accessibility(&self) -> Result<()>;
}

/// Read selected text from the active application (future TTS)
pub trait TextSelector: Send + Sync {
    fn get_selected_text(&self) -> Result<Option<String>>;
    fn is_supported(&self) -> bool;
}

/// Get the platform text injector
pub fn get_text_injector() -> Box<dyn TextInjector> {
    #[cfg(target_os = "macos")]
    {
        Box::new(MacOsTextInjector::new())
    }
    #[cfg(not(target_os = "macos"))]
    {
        panic!("Text injection not supported on this platform")
    }
}
