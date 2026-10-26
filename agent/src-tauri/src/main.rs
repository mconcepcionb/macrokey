#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actions;
mod commands;
mod config;
mod context;
mod db;
mod engine;
mod input;
mod logging;
mod osd;
mod tray;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, PhysicalPosition};
use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};

pub(crate) fn fatal(title: &str, message: &str) {
    crate::log_error!("{title}: {message}");
    let text: Vec<u16> = format!("{message}\0").encode_utf16().collect();
    let caption: Vec<u16> = format!("{title}\0").encode_utf16().collect();
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            caption.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

fn center_on_monitor(window: &tauri::WebviewWindow) {
    let Ok(size) = window.outer_size() else {
        return;
    };
    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return;
    };
    let monitor_size = monitor.size();
    let monitor_position = monitor.position();
    let x = monitor_position.x + (monitor_size.width as i32 - size.width as i32).max(0) / 2;
    let y = monitor_position.y + (monitor_size.height as i32 - size.height as i32).max(0) / 2;
    crate::log_info!(
        "[settings] size={}x{} monitor={}x{}@{} -> {x},{y}",
        size.width,
        size.height,
        monitor_size.width,
        monitor_size.height,
        monitor_position.x
    );
    let _ = window.set_position(PhysicalPosition::new(x, y));
}

fn show_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        center_on_monitor(&window);
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn install_tray(app: &tauri::App) -> tauri::Result<()> {
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let legend = MenuItem::with_id(app, "legend", "Show current layer", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&settings, &legend, &quit])?;

    TrayIconBuilder::with_id("macrokey")
        .icon(tray::icon("#7dd3fc"))
        .tooltip("macrokey")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "settings" => show_settings(app),
            "legend" => engine::request_legend(),
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;

    Ok(())
}

fn main() {
    let db_path = db::db_path();
    logging::init(&db_path.with_file_name("agent.log"));
    crate::log_info!(
        "macrokey agent starting; db={} log={}",
        db_path.display(),
        logging::path()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "(console)".into())
    );

    // A fresh database is the only moment where the app has no visible
    // surface at all. Open Settings once so a first-time user can find it.
    let first_run = !db_path.exists();
    let runtime = match config::load_runtime() {
        Ok(runtime) => runtime,
        Err(error) => {
            fatal(
                "macrokey",
                &format!(
                    "Could not load the configuration.\n\n{error:#}\n\nDatabase: {}",
                    db_path.display()
                ),
            );
            std::process::exit(1);
        }
    };

    let result = tauri::Builder::default()
        // Registered first: a second launch asks the running instance to show
        // Settings and then exits instead of opening the pad twice.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_settings(app);
        }))
        .invoke_handler(tauri::generate_handler![
            commands::load_config,
            commands::save_config,
            commands::reset_config,
            commands::export_config,
            commands::current_app,
            commands::reveal_config,
            commands::show_osd,
        ])
        .setup(move |app| {
            if let Some(window) = app.get_webview_window("osd") {
                osd::init(&window);
            }
            if let Some(window) = app.get_webview_window("settings") {
                let handle = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = handle.hide();
                    }
                });
            }
            install_tray(app)?;
            engine::spawn(app.handle().clone(), runtime);

            let open_settings =
                first_run || std::env::var_os("MACROKEY_SETTINGS").is_some();
            if open_settings {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(900));
                    show_settings(&handle);
                });
            }

            if std::env::var_os("MACROKEY_DEMO").is_some() {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(1400));
                    let payload = osd::Payload {
                        context: "VS Code",
                        layer: "Debug",
                        color: "#63b3ed",
                        keys: ["Ctrl+S", "Ctrl+Shift+P", "Ctrl+`"],
                        knob: ["Scroll ↓", "Build", "Scroll ↑"],
                        badge: Some("manual"),
                        pressed: Some("k2"),
                        action: Some("Ctrl+Shift+P"),
                        rail: serde_json::json!([
                            {"id": "edit", "label": "Edit", "active": false},
                            {"id": "nav", "label": "Nav", "active": false},
                            {"id": "debug", "label": "Debug", "active": true},
                            {"id": "build", "label": "Build", "active": false}
                        ]),
                        mode: "press",
                        gestures: serde_json::json!({
                            "k1": {"tap": "Ctrl+S", "double": "Save all", "hold": ""},
                            "k2": {"tap": "Ctrl+Shift+P", "double": "", "hold": ""},
                            "k3": {"tap": "Ctrl+`", "double": "Nav (next key)", "hold": "Nav (held)"},
                            "knob_ccw": {"tap": "Scroll ↓", "double": "", "hold": ""},
                            "knob_press": {"tap": "Build", "double": "Next layer", "hold": "Peek legend"},
                            "knob_cw": {"tap": "Scroll ↑", "double": "", "hold": ""}
                        }),
                    }
                    .into_json();
                    osd::show(&handle, payload, 0, true, "primary");
                });
            }

            Ok(())
        })
        .run(tauri::generate_context!());

    if let Err(error) = result {
        fatal(
            "macrokey",
            &format!("The macrokey agent could not start.\n\n{error}"),
        );
        std::process::exit(1);
    }
}
