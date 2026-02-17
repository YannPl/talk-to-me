use anyhow::Result;
use super::{TextInjector, TextSelector};

pub struct MacOsTextInjector;

impl MacOsTextInjector {
    pub fn new() -> Self {
        Self
    }
}

impl TextInjector for MacOsTextInjector {
    fn inject_text(&self, text: &str) -> Result<()> {
        self.inject_via_clipboard(text)
    }

    fn inject_via_clipboard(&self, text: &str) -> Result<()> {
        use std::process::Command;

        let mut child = Command::new("pbcopy")
            .stdin(std::process::Stdio::piped())
            .spawn()?;

        if let Some(ref mut stdin) = child.stdin {
            use std::io::Write;
            stdin.write_all(text.as_bytes())?;
        }
        child.wait()?;

        Command::new("osascript")
            .arg("-e")
            .arg("tell application \"System Events\" to keystroke \"v\" using command down")
            .output()?;

        Ok(())
    }

    fn is_accessibility_granted(&self) -> bool {
        true
    }

    fn request_accessibility(&self) -> Result<()> {
        std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .spawn()?;
        Ok(())
    }
}

pub struct MacOsTextSelector;

impl MacOsTextSelector {
    pub fn new() -> Self {
        Self
    }
}

impl TextSelector for MacOsTextSelector {
    fn get_selected_text(&self) -> Result<Option<String>> {
        todo!("macOS text selection via Accessibility API - Phase 6 (TTS)")
    }

    fn is_supported(&self) -> bool {
        true
    }
}
