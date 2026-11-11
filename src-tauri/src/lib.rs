//! OpsOpsAi backend: app assembly, IPC commands, managed state.
//!
//! Каркас Phase 1: диагностика, настройки, права (Entitlements). Runtime локального ИИ, модели и
//! чат появятся в Phase 2 за отдельными модулями (см. docs/ARCHITECTURE.md).

pub mod diag;
pub mod entitlements;
pub mod store;

use serde::Serialize;

#[derive(Serialize)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub os: &'static str,
    pub arch: &'static str,
}

#[tauri::command]
fn app_info(app: tauri::AppHandle) -> AppInfo {
    let pkg = app.package_info();
    AppInfo {
        name: pkg.name.clone(),
        version: pkg.version.to_string(),
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
    }
}

pub fn run() {
    diag::install_panic_hook();
    tauri::Builder::default()
        .plugin(diag::log_plugin())
        .setup(|app| {
            diag::start_watchdog(app.handle().clone());
            log::info!("OpsOpsAi {} started on {}/{}", app.package_info().version, std::env::consts::OS, std::env::consts::ARCH);
            Ok(())
        })
        .invoke_handler(diag::track(tauri::generate_handler![
            app_info,
            entitlements::entitlements_get,
            diag::log_ui,
            diag::logs_open,
            diag::logs_tail,
            diag::logs_path,
        ]))
        .run(tauri::generate_context!())
        .expect("error while running OpsOpsAi");
}
