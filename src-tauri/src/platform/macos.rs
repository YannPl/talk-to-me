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
        // Phase 4: Full CGEvent implementation
        // For now, fall back to clipboard
        self.inject_via_clipboard(text)
    }

    fn inject_via_clipboard(&self, text: &str) -> Result<()> {
        use std::process::Command;

        // Copy text to clipboard using pbcopy
        let mut child = Command::new("pbcopy")
            .stdin(std::process::Stdio::piped())
            .spawn()?;

        if let Some(ref mut stdin) = child.stdin {
            use std::io::Write;
            stdin.write_all(text.as_bytes())?;
        }
        child.wait()?;

        // Simulate Cmd+V to paste
        // For now we use osascript as a simple approach
        // Phase 4 will use CGEvent directly
        Command::new("osascript")
            .arg("-e")
            .arg("tell application \"System Events\" to keystroke \"v\" using command down")
            .output()?;

        Ok(())
    }

    fn is_accessibility_granted(&self) -> bool {
        // Check using AXIsProcessTrusted
        // For now return true; Phase 4 will implement proper check
        true
    }

    fn request_accessibility(&self) -> Result<()> {
        // Phase 4: Open System Preferences > Privacy > Accessibility
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
