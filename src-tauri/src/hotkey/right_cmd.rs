use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::OnceLock;
use tauri::AppHandle;

type CGEventTapProxy = *mut std::ffi::c_void;
type CGEventRef = *mut std::ffi::c_void;
type CFMachPortRef = *mut std::ffi::c_void;
type CFRunLoopSourceRef = *mut std::ffi::c_void;
type CFRunLoopRef = *mut std::ffi::c_void;
type CFStringRef = *const std::ffi::c_void;

type CGEventType = u32;
type CGEventMask = u64;
type CGEventTapLocation = u32;
type CGEventTapPlacement = u32;
type CGEventTapOptions = u32;

const K_CG_SESSION_EVENT_TAP: CGEventTapLocation = 1;
const K_CG_HEAD_INSERT_EVENT_TAP: CGEventTapPlacement = 0;
const K_CG_EVENT_TAP_OPTION_LISTEN_ONLY: CGEventTapOptions = 1;
const K_CG_EVENT_FLAGS_CHANGED: CGEventType = 12;
const K_CG_EVENT_FLAG_MASK_COMMAND: u64 = 1 << 20;
const NX_DEVICERCMDKEYMASK: u64 = 0x10;  // Right Command in device-dependent flags

type CGEventTapCallBack = unsafe extern "C" fn(
    CGEventTapProxy,
    CGEventType,
    CGEventRef,
    *mut std::ffi::c_void,
) -> CGEventRef;

extern "C" {
    fn CGEventTapCreate(
        tap: CGEventTapLocation,
        place: CGEventTapPlacement,
        options: CGEventTapOptions,
        events_of_interest: CGEventMask,
        callback: CGEventTapCallBack,
        user_info: *mut std::ffi::c_void,
    ) -> CFMachPortRef;
    fn CGEventGetFlags(event: CGEventRef) -> u64;
    fn CFMachPortCreateRunLoopSource(
        allocator: *const std::ffi::c_void,
        port: CFMachPortRef,
        order: i64,
    ) -> CFRunLoopSourceRef;
    fn CFRunLoopGetCurrent() -> CFRunLoopRef;
    fn CFRunLoopAddSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);
    fn CFRunLoopRun();
    fn CFRunLoopStop(rl: CFRunLoopRef);
    fn CFRelease(cf: *const std::ffi::c_void);
    fn CFMachPortInvalidate(port: CFMachPortRef);

    static kCFRunLoopCommonModes: CFStringRef;
}

static RUNNING: AtomicBool = AtomicBool::new(false);
static RUN_LOOP_REF: AtomicPtr<std::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());
static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();
static CMD_DOWN: AtomicBool = AtomicBool::new(false);

unsafe extern "C" fn tap_callback(
    _proxy: CGEventTapProxy,
    event_type: CGEventType,
    event: CGEventRef,
    _user_info: *mut std::ffi::c_void,
) -> CGEventRef {
    if event_type != K_CG_EVENT_FLAGS_CHANGED {
        return event;
    }

    let flags = unsafe { CGEventGetFlags(event) };
    let right_cmd_now = (flags & K_CG_EVENT_FLAG_MASK_COMMAND) != 0
        && (flags & NX_DEVICERCMDKEYMASK) != 0;
    let was_down = CMD_DOWN.load(Ordering::SeqCst);

    if right_cmd_now && !was_down {
        CMD_DOWN.store(true, Ordering::SeqCst);
        if let Some(app) = APP_HANDLE.get() {
            let _ = super::handle_hotkey(
                app,
                super::HotkeyAction::ToggleStt,
                tauri_plugin_global_shortcut::ShortcutState::Pressed,
            );
        }
    } else if !right_cmd_now && was_down {
        CMD_DOWN.store(false, Ordering::SeqCst);
        if let Some(app) = APP_HANDLE.get() {
            let _ = super::handle_hotkey(
                app,
                super::HotkeyAction::ToggleStt,
                tauri_plugin_global_shortcut::ShortcutState::Released,
            );
        }
    }

    event
}

pub fn start_right_cmd_tap(app_handle: &AppHandle) -> anyhow::Result<()> {
    if RUNNING.load(Ordering::SeqCst) {
        return Ok(());
    }

    let _ = APP_HANDLE.set(app_handle.clone());

    let event_mask: CGEventMask = 1 << K_CG_EVENT_FLAGS_CHANGED;
    let (tx, rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        unsafe {
            let tap = CGEventTapCreate(
                K_CG_SESSION_EVENT_TAP,
                K_CG_HEAD_INSERT_EVENT_TAP,
                K_CG_EVENT_TAP_OPTION_LISTEN_ONLY,
                event_mask,
                tap_callback,
                std::ptr::null_mut(),
            );

            if tap.is_null() {
                tracing::error!(
                    "CGEventTapCreate returned null — Accessibility permission is likely not granted"
                );
                let _ = tx.send(false);
                return;
            }

            let source =
                CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0);
            let run_loop = CFRunLoopGetCurrent();

            RUN_LOOP_REF.store(run_loop, Ordering::SeqCst);
            RUNNING.store(true, Ordering::SeqCst);

            CFRunLoopAddSource(run_loop, source, kCFRunLoopCommonModes);
            let _ = tx.send(true);
            CFRunLoopRun();

            // Cleanup after CFRunLoopStop
            CFMachPortInvalidate(tap);
            CFRelease(tap);
            CFRelease(source);
            RUNNING.store(false, Ordering::SeqCst);
            RUN_LOOP_REF.store(std::ptr::null_mut(), Ordering::SeqCst);
        }
    });

    match rx.recv_timeout(std::time::Duration::from_secs(2)) {
        Ok(true) => Ok(()),
        Ok(false) => {
            anyhow::bail!("Right Command requires Accessibility permission. Please grant it in System Settings > Privacy & Security > Accessibility.")
        }
        Err(_) => {
            anyhow::bail!("Timeout waiting for Right Command event tap creation")
        }
    }
}

pub fn stop_right_cmd_tap() {
    if !RUNNING.load(Ordering::SeqCst) {
        return;
    }
    let rl = RUN_LOOP_REF.load(Ordering::SeqCst);
    if !rl.is_null() {
        unsafe { CFRunLoopStop(rl) };
    }
    CMD_DOWN.store(false, Ordering::SeqCst);
}
