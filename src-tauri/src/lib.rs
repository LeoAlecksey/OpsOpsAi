//! OpsOpsAi backend: app assembly, IPC commands, managed state.
//!
//! Каркас Phase 1: диагностика и настройки. Runtime локального ИИ, модели и
//! чат появятся в Phase 2 за отдельными модулями.

pub mod diag;
pub mod store;
pub mod updater;

use serde::Serialize;

#[derive(Serialize)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub os: &'static str,
    pub arch: &'static str,
}

/// The author's YooMoney wallet for donations. Fixed here on purpose: the UI only asks to open
/// "the donation page" and cannot pass a URL of its own, so a bug or injected script in the
/// webview cannot send people to a different wallet.
pub const DONATE_URL: &str = "https://yoomoney.ru/to/4100119645604976";

#[tauri::command]
fn donate_open() -> Result<(), String> {
    store::open_with_system(DONATE_URL)
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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            diag::start_watchdog(app.handle().clone());
            log::info!("OpsOpsAi {} started on {}/{}", app.package_info().version, std::env::consts::OS, std::env::consts::ARCH);
            Ok(())
        })
        .invoke_handler(diag::track(tauri::generate_handler![
            app_info,
            donate_open,
            updater::update_check,
            updater::update_install,
            diag::log_ui,
            diag::logs_open,
            diag::logs_tail,
            diag::logs_path,
        ]))
        .run(tauri::generate_context!())
        .expect("error while running OpsOpsAi");
}

#[cfg(test)]
mod tests {
    /// Guard against an accidental or malicious change of the wallet: changing it must be a
    /// deliberate edit of this test as well.
    #[test]
    fn the_donation_link_is_the_authors_wallet() {
        assert_eq!(super::DONATE_URL, "https://yoomoney.ru/to/4100119645604976");
    }
}
