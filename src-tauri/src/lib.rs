mod commands;

use auto_manager_core::{Core, Paths, logging, state};
use tauri::Manager as _;

fn fatal(what: &str, e: impl std::fmt::Display) -> ! {
    eprintln!("auto-manager: {what}: {e}");
    tracing::error!("{what}: {e}");
    std::process::exit(1);
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
            app.manage(commands::system::Updates::default());
            Ok(())
        })
        .invoke_handler(include!(concat!(env!("OUT_DIR"), "/handlers.rs")))
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| fatal("cannot start the user interface", e));
}

struct LogGuard(#[allow(dead_code)] logging::WorkerGuard);
