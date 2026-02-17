#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;

use anyhow::Result;

pub trait TextInjector: Send + Sync {
    fn inject_text(&self, text: &str) -> Result<()>;
    fn inject_via_clipboard(&self, text: &str) -> Result<()>;
    fn is_accessibility_granted(&self) -> bool;
    fn request_accessibility(&self) -> Result<()>;
}

pub trait TextSelector: Send + Sync {
    fn get_selected_text(&self) -> Result<Option<String>>;
    fn is_supported(&self) -> bool;
}

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
