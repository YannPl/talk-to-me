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
type CGEventField = u32;
type CGEventTapLocation = u32;
type CGEventTapPlacement = u32;
type CGEventTapOptions = u32;

const K_CG_SESSION_EVENT_TAP: CGEventTapLocation = 1;
const K_CG_HEAD_INSERT_EVENT_TAP: CGEventTapPlacement = 0;
const K_CG_EVENT_TAP_OPTION_LISTEN_ONLY: CGEventTapOptions = 1;
const K_CG_EVENT_FLAGS_CHANGED: CGEventType = 12;
const K_CG_KEYBOARD_EVENT_KEYCODE: CGEventField = 6;
const K_CG_EVENT_FLAG_MASK_COMMAND: u64 = 1 << 20;
const RIGHT_COMMAND_KEYCODE: i64 = 54;

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
    fn CGEventGetIntegerValueField(event: CGEventRef, field: CGEventField) -> i64;
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

    let keycode = unsafe { CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_KEYCODE) };
    if keycode != RIGHT_COMMAND_KEYCODE {
        // Not the right command key — reset our tracking
        if CMD_DOWN.load(Ordering::SeqCst) {
            CMD_DOWN.store(false, Ordering::SeqCst);
        }
        return event;
    }

    let flags = unsafe { CGEventGetFlags(event) };
    let cmd_pressed = (flags & K_CG_EVENT_FLAG_MASK_COMMAND) != 0;
    let was_down = CMD_DOWN.load(Ordering::SeqCst);

    if cmd_pressed && !was_down {
        CMD_DOWN.store(true, Ordering::SeqCst);
        if let Some(app) = APP_HANDLE.get() {
            let _ = super::handle_hotkey(
                app,
                super::HotkeyAction::ToggleStt,
                tauri_plugin_global_shortcut::ShortcutState::Pressed,
            );
        }
    } else if !cmd_pressed && was_down {
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
                    "Failed to create CGEventTap for Right Command. \
                     Accessibility permission may be required."
                );
                return;
            }

            let source =
                CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0);
            let run_loop = CFRunLoopGetCurrent();

            RUN_LOOP_REF.store(run_loop, Ordering::SeqCst);
            RUNNING.store(true, Ordering::SeqCst);

            CFRunLoopAddSource(run_loop, source, kCFRunLoopCommonModes);
            tracing::info!("Right Command event tap started");
            CFRunLoopRun();

            // Cleanup after CFRunLoopStop
            CFMachPortInvalidate(tap);
            CFRelease(tap);
            CFRelease(source);
            RUNNING.store(false, Ordering::SeqCst);
            RUN_LOOP_REF.store(std::ptr::null_mut(), Ordering::SeqCst);
            tracing::info!("Right Command event tap stopped");
        }
    });

    Ok(())
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
