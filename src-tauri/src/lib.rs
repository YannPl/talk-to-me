mod audio;
mod commands;
mod engine;
mod hotkey;
mod hub;
mod persistence;
mod platform;
mod state;

use state::AppState;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt::init();

    tracing::info!("Starting Talk to Me v{}", env!("CARGO_PKG_VERSION"));

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::stt::start_recording,
            commands::stt::stop_recording,
            commands::stt::get_status,
            commands::models::list_installed_models,
            commands::models::get_catalog,
            commands::models::download_model,
            commands::models::delete_model,
            commands::models::cancel_download,
            commands::models::set_active_model,
            commands::models::get_active_model,
            commands::tts::speak_selected_text,
            commands::tts::speak_text,
            commands::tts::stop_speaking,
            commands::settings::get_settings,
            commands::settings::update_settings,
            commands::settings::check_accessibility_permission,
            commands::settings::request_accessibility_permission,
            commands::settings::get_app_version,
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            {
                use objc2_app_kit::NSApplication;
                use objc2_app_kit::NSApplicationActivationPolicy;
                // Safety: Tauri's setup closure runs on the main thread
                let mtm = unsafe { objc2::MainThreadMarker::new_unchecked() };
                let ns_app = NSApplication::sharedApplication(mtm);
                ns_app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
            }

            let show_settings = MenuItem::with_id(
                app,
                "show_settings",
                "Preferences...",
                true,
                Some("CmdOrCtrl+,"),
            )?;
            let manage_models =
                MenuItem::with_id(app, "manage_models", "Manage Models...", true, None::<&str>)?;
            let about = MenuItem::with_id(app, "about", "About Talk to Me", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, Some("CmdOrCtrl+Q"))?;

            let stt_header =
                MenuItem::with_id(app, "stt_header", "Dictation (STT)", false, None::<&str>)?;
            let stt_model = MenuItem::with_id(
                app,
                "stt_model",
                "  Model: None selected",
                false,
                None::<&str>,
            )?;
            let stt_shortcut = MenuItem::with_id(
                app,
                "stt_shortcut",
                "  Shortcut: \u{2325}Space",
                false,
                None::<&str>,
            )?;

            let tts_header = MenuItem::with_id(
                app,
                "tts_header",
                "Read Aloud (TTS) \u{2014} Coming Soon",
                false,
                None::<&str>,
            )?;

            let separator1 = PredefinedMenuItem::separator(app)?;
            let separator2 = PredefinedMenuItem::separator(app)?;
            let separator3 = PredefinedMenuItem::separator(app)?;

            let menu = Menu::with_items(
                app,
                &[
                    &stt_header,
                    &stt_model,
                    &stt_shortcut,
                    &separator1,
                    &tts_header,
                    &separator2,
                    &show_settings,
                    &manage_models,
                    &separator3,
                    &about,
                    &quit,
                ],
            )?;

            let tray_icon_bytes = include_bytes!("../icons/tray-icon.png");
            let tray_icon =
                tauri::image::Image::from_bytes(tray_icon_bytes).expect("Failed to load tray icon");
            let _tray = TrayIconBuilder::new()
                .icon(tray_icon)
                .icon_as_template(true)
                .menu(&menu)
                .tooltip("Talk to Me")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show_settings" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                        let _ = app.emit("navigate-tab", "general");
                    }
                    "manage_models" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                        let _ = app.emit("navigate-tab", "models");
                    }
                    "about" => {
                        tracing::info!("Talk to Me v{}", env!("CARGO_PKG_VERSION"));
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            {
                let loaded = persistence::load_settings(app.handle());
                let state = app.state::<AppState>();
                *state.settings.lock().unwrap() = loaded;
                tracing::info!("Settings loaded from store");
            }

            {
                let state = app.state::<AppState>();
                let active_model_id = state.settings.lock().unwrap().stt.active_model_id.clone();

                if let Some(ref model_id) = active_model_id {
                    let installed = hub::registry::list_installed_models(Some(
                        &engine::ModelCapability::SpeechToText,
                    ));
                    match installed {
                        Ok(models) if models.iter().any(|m| m.id == *model_id) => {
                            match commands::models::load_stt_engine(app.handle(), model_id) {
                                Ok(()) => tracing::info!("Auto-loaded STT model: {}", model_id),
                                Err(e) => {
                                    tracing::warn!(
                                        "Failed to auto-load STT model '{}': {}. Clearing.",
                                        model_id,
                                        e
                                    );
                                    state.settings.lock().unwrap().stt.active_model_id = None;
                                    persistence::save_settings(app.handle());
                                }
                            }
                        }
                        Ok(_) => {
                            tracing::warn!(
                                "Previously active STT model '{}' no longer installed. Clearing.",
                                model_id
                            );
                            state.settings.lock().unwrap().stt.active_model_id = None;
                            persistence::save_settings(app.handle());
                        }
                        Err(e) => {
                            tracing::warn!("Failed to list installed models at startup: {}", e);
                        }
                    }
                }
            }

            use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

            let shortcut: Shortcut = "Alt+Space".parse().expect("Failed to parse shortcut");
            let app_handle = app.handle().clone();

            app.global_shortcut()
                .on_shortcut(shortcut, move |_app, _shortcut, event| {
                    if let Err(e) = hotkey::handle_hotkey(
                        &app_handle,
                        hotkey::HotkeyAction::ToggleStt,
                        event.state,
                    ) {
                        tracing::error!("Hotkey error: {}", e);
                    }
                })?;

            tracing::info!("Global shortcut registered: Alt+Space for STT");

            if let Some(window) = app.get_webview_window("main") {
                let w = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = w.hide();
                    }
                });
            }

            tracing::info!("App setup complete");

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
