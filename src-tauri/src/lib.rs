mod commands;

use auto_manager_core::{Core, Paths, logging, state};
use tauri::Manager as _;

fn fatal(what: &str, e: impl std::fmt::Display) -> ! {
    eprintln!("auto-manager: {what}: {e}");
    tracing::error!("{what}: {e}");
    std::process::exit(1);
}

pub fn self_test() -> i32 {
    let Some(dir) = std::env::var_os("AUTO_MANAGER_DATA_DIR") else {
        eprintln!("self-test: set AUTO_MANAGER_DATA_DIR to a scratch folder");
        return 2;
    };
    let result = Core::open(Paths::new(dir.into())).and_then(|core| {
        core.read(|c| {
            let ok: String = c.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
            Ok((ok, auto_manager_core::db::user_version(c)?))
        })
    });
    match result {
        Ok((ok, v)) if ok == "ok" => {
            println!("{{\"version\":\"{}\",\"schema\":{v},\"integrity\":\"ok\"}}", env!("CARGO_PKG_VERSION"));
            0
        }
        Ok((ok, _)) => {
            eprintln!("self-test: integrity check failed: {ok}");
            1
        }
        Err(e) => {
            eprintln!("self-test: {e}");
            1
        }
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let platform = app.path().app_data_dir().unwrap_or_else(|e| fatal("cannot find the application data folder", e));
            let paths = Paths::new(state::data_root(&platform));
            if let Err(e) = paths.ensure() {
                fatal("cannot create the data folder", e);
            }
            match logging::init(&paths.logs()) {
                Ok(guard) => {
                    // Keep the log writer alive for the life of the process.
                    app.manage(LogGuard(guard));
                }
                Err(e) => eprintln!("auto-manager: logging disabled: {e}"),
            }
            tracing::info!(version = env!("CARGO_PKG_VERSION"), "starting");
            let core = Core::open(paths).unwrap_or_else(|e| fatal("cannot open the database", e));
            app.manage(core);
            app.manage(commands::quickbooks::Qbo::default());
            // Text-recognition models for reading VINs from photos (bundled resources; in
            // development, straight from src-tauri/resources).
            let ocr = app.path().resource_dir().map(|d| d.join("ocr")).ok().filter(|d| d.join("text-recognition.onnx").exists());
            auto_manager_core::scan::set_models_dir(
                ocr.unwrap_or_else(|| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/ocr")),
            );
            if let Some(w) = app.get_webview_window("main") {
                camera::enable(&w);
            }
            Ok(())
        })
        .invoke_handler(include!(concat!(env!("OUT_DIR"), "/handlers.rs")))
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| fatal("cannot start the user interface", e));
}

struct LogGuard(#[allow(dead_code)] logging::WorkerGuard);

/// Let the page use a camera for scanning VINs and barcodes. Windows (WebView2) and macOS
/// (WKWebView, with NSCameraUsageDescription in Info.plist) ask the person themselves; WebKitGTK
/// needs media capture switched on and its permission requests answered here: video only, never
/// the microphone, and only for this app's own pages (the CSP allows nothing else to load).
mod camera {
    #[cfg(target_os = "linux")]
    pub fn enable(w: &tauri::WebviewWindow) {
        let _ = w.with_webview(|wv| {
            use webkit2gtk::glib::object::Cast;
            use webkit2gtk::{PermissionRequestExt, SettingsExt, UserMediaPermissionRequest, UserMediaPermissionRequestExt, WebViewExt};
            let view = wv.inner();
            if let Some(settings) = WebViewExt::settings(&view) {
                settings.set_enable_media_stream(true);
                // End-to-end tests run without a camera; WebKit's mock device stands in for one.
                if std::env::var_os("AUTO_MANAGER_MOCK_CAMERA").is_some() {
                    settings.set_enable_mock_capture_devices(true);
                }
            }
            view.connect_permission_request(|_, req| match req.downcast_ref::<UserMediaPermissionRequest>() {
                Some(m) if m.is_for_video_device() && !m.is_for_audio_device() => {
                    req.allow();
                    true
                }
                _ => {
                    req.deny();
                    true
                }
            });
        });
    }

    #[cfg(not(target_os = "linux"))]
    pub fn enable(_: &tauri::WebviewWindow) {}
}
