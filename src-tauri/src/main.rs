// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `auto-manager --self-test` (with AUTO_MANAGER_DATA_DIR set) opens and migrates the database,
    // checks its integrity and exits, without a window. Release CI runs it on every platform.
    if std::env::args().any(|a| a == "--self-test") {
        std::process::exit(auto_manager_lib::self_test());
    }
    auto_manager_lib::run();
}
