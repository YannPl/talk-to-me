use anyhow::Result;
use super::{TextInjector, TextSelector, MediaController};

type CGEventRef = *mut std::ffi::c_void;

extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn CGEventCreateKeyboardEvent(
        source: *const std::ffi::c_void,
        virtual_key: u16,
        key_down: bool,
    ) -> CGEventRef;
    fn CGEventSetFlags(event: CGEventRef, flags: u64);
    fn CGEventPost(tap: u32, event: CGEventRef);
    fn CFRelease(cf: *const std::ffi::c_void);
}

const K_VK_V: u16 = 9;
const K_CG_EVENT_FLAG_MASK_COMMAND: u64 = 1 << 20;
const K_CG_HID_EVENT_TAP: u32 = 0;

fn simulate_cmd_v() -> Result<()> {
    unsafe {
        let key_down = CGEventCreateKeyboardEvent(std::ptr::null(), K_VK_V, true);
        if key_down.is_null() {
            anyhow::bail!("Failed to create CGEvent for Cmd+V — grant Accessibility permission");
        }
        CGEventSetFlags(key_down, K_CG_EVENT_FLAG_MASK_COMMAND);
        CGEventPost(K_CG_HID_EVENT_TAP, key_down);

        let key_up = CGEventCreateKeyboardEvent(std::ptr::null(), K_VK_V, false);
        if !key_up.is_null() {
            CGEventSetFlags(key_up, K_CG_EVENT_FLAG_MASK_COMMAND);
            CGEventPost(K_CG_HID_EVENT_TAP, key_up);
            CFRelease(key_up as *const _);
        }

        CFRelease(key_down as *const _);
    }
    Ok(())
}

fn copy_to_clipboard(text: &str) -> Result<()> {
    use std::process::Command;
    use std::io::Write;

    let mut child = Command::new("pbcopy")
        .env("LANG", "en_US.UTF-8")
        .stdin(std::process::Stdio::piped())
        .spawn()?;
    if let Some(ref mut stdin) = child.stdin {
        stdin.write_all(text.as_bytes())?;
    }
    child.wait()?;
    Ok(())
}

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
        copy_to_clipboard(text)?;
        let trusted = self.is_accessibility_granted();
        tracing::info!("AXIsProcessTrusted() = {}, attempting text injection ({} chars)", trusted, text.len());
        if trusted {
            std::thread::sleep(std::time::Duration::from_millis(50));
            if let Err(e) = simulate_cmd_v() {
                tracing::warn!("CGEvent Cmd+V failed: {}. Text is in clipboard.", e);
            } else {
                tracing::info!("Cmd+V simulated successfully");
            }
        } else {
            tracing::warn!("Accessibility not granted — text copied to clipboard but cannot auto-paste. Grant permission in System Settings > Privacy & Security > Accessibility.");
        }
        Ok(())
    }

    fn is_accessibility_granted(&self) -> bool {
        unsafe { AXIsProcessTrusted() }
    }

    fn request_accessibility(&self) -> Result<()> {
        std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .spawn()?;
        Ok(())
    }
}

use std::ffi::c_void;
use std::sync::OnceLock;

const _MR_COMMAND_PLAY: u32 = 0;
const MR_COMMAND_PAUSE: u32 = 1;

type MRSendCommandFn = unsafe extern "C" fn(command: u32, options: *const c_void) -> bool;

struct MediaRemote {
    send_command: MRSendCommandFn,
}

static MEDIA_REMOTE: OnceLock<Option<MediaRemote>> = OnceLock::new();

fn media_remote() -> Option<&'static MediaRemote> {
    MEDIA_REMOTE.get_or_init(|| {
        unsafe {
            let path = c"/System/Library/PrivateFrameworks/MediaRemote.framework/MediaRemote";
            let handle = libc::dlopen(path.as_ptr(), libc::RTLD_LAZY);
            if handle.is_null() {
                tracing::warn!("Failed to load MediaRemote framework");
                return None;
            }
            let sym = libc::dlsym(handle, c"MRMediaRemoteSendCommand".as_ptr());
            if sym.is_null() {
                tracing::warn!("MRMediaRemoteSendCommand not found");
                return None;
            }
            tracing::info!("MediaRemote framework loaded");
            Some(MediaRemote {
                send_command: std::mem::transmute(sym),
            })
        }
    }).as_ref()
}

pub struct MacOsMediaController;

static MEDIA_CONTROLLER: OnceLock<MacOsMediaController> = OnceLock::new();

impl MacOsMediaController {
    pub fn instance() -> &'static Self {
        MEDIA_CONTROLLER.get_or_init(|| MacOsMediaController)
    }
}

impl MediaController for MacOsMediaController {
    fn pause_if_playing(&self) {
        if let Some(mr) = media_remote() {
            let ok = unsafe { (mr.send_command)(MR_COMMAND_PAUSE, std::ptr::null()) };
            tracing::info!("MediaRemote pause sent (ok={})", ok);
        }
    }

    fn resume(&self) {
        // No-op: we intentionally don't resume media. Sending play would start
        // music even when nothing was playing before recording, which is worse
        // than leaving paused media paused.
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
