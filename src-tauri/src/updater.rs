//! In-app updates from GitHub Releases (tauri-plugin-updater). Releases are signed in CI with
//! the project's private key; the app only installs packages whose signature matches the public
//! key in tauri.conf.json.
//!
//! Origin: adapted from OpsDeck `src-tauri/src/updater.rs` (same author).

use crate::store::err;
use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::UpdaterExt;

/// Where users get packages by hand (same repository as the updater endpoint in tauri.conf.json).
const RELEASES_PAGE: &str = "https://github.com/LeoAlecksey/OpsOpsAi/releases/latest";

#[derive(Serialize)]
pub struct UpdateInfo {
    current: String,
    available: bool,
    version: Option<String>,
    notes: Option<String>,
}

fn check_err(e: tauri_plugin_updater::Error) -> String {
    explain_check_error(&e.to_string())
}

/// What a failed check means for the user, in words.
fn explain_check_error(e: &str) -> String {
    // no published release yet (or no latest.json in it) — not an error for the user
    if e.contains("404") || e.contains("valid release JSON") {
        "опубликованных обновлений пока нет".to_string()
    } else if e.contains("pubkey") || e.contains("public key") {
        "в этой сборке не настроена проверка подписи обновлений".to_string()
    } else {
        format!("не удалось проверить обновления: {e}")
    }
}

/// Why the installation failed — in words, not a library error code.
fn explain_install_error(e: &str) -> String {
    // a copy installed from .deb/.rpm cannot take an AppImage (and the other way round)
    if e.contains("invalid updater binary format") || e.contains("not a valid deb") {
        return format!(
            "эта копия установлена другим типом пакета, чем пришло обновление. \
Скачайте пакет со страницы релиза и установите его вручную: {RELEASES_PAGE}"
        );
    }
    if e.contains("signature") {
        return "подпись обновления не совпала — установка отменена".to_string();
    }
    format!("не удалось установить обновление: {e}")
}

fn semver_gt(a: &str, b: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> { s.split(['.', '-', '+']).take(3).map(|p| p.parse().unwrap_or(0)).collect() };
    parse(a) > parse(b)
}

#[tauri::command]
pub async fn update_check(app: AppHandle) -> Result<UpdateInfo, String> {
    let current = app.package_info().version.to_string();
    let update = app.updater().map_err(err)?.check().await.map_err(check_err)?;
    match update {
        Some(u) if semver_gt(&u.version, &current) => Ok(UpdateInfo {
            current,
            available: true,
            version: Some(u.version.clone()),
            notes: u.body.clone(),
        }),
        _ => Ok(UpdateInfo { current, available: false, version: None, notes: None }),
    }
}

/// Downloads, verifies the signature, installs and restarts. Progress: `update-progress` events.
#[tauri::command]
pub async fn update_install(app: AppHandle) -> Result<(), String> {
    let update = app
        .updater()
        .map_err(err)?
        .check()
        .await
        .map_err(check_err)?
        .ok_or("обновлений нет — установлена последняя версия")?;
    let mut done: u64 = 0;
    let progress = app.clone();
    let finished = app.clone();
    update
        .download_and_install(
            move |chunk, total| {
                done += chunk as u64;
                let _ = progress.emit("update-progress", json!({ "downloaded": done, "total": total }));
            },
            move || {
                let _ = finished.emit("update-progress", json!({ "installing": true }));
            },
        )
        .await
        .map_err(|e| explain_install_error(&e.to_string()))?;
    log::info!("update {} installed, restarting", update.version);
    app.restart();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_by_numbers_not_text() {
        assert!(semver_gt("0.10.0", "0.9.9"));
        assert!(semver_gt("1.0.0", "0.99.0"));
        assert!(semver_gt("0.1.1", "0.1.0"));
        assert!(!semver_gt("0.1.0", "0.1.0"));
        assert!(!semver_gt("0.1.0", "0.2.0"));
        assert!(!semver_gt("0.2.0-beta", "0.2.0"));
    }

    #[test]
    fn no_release_yet_is_not_an_error_for_the_user() {
        assert_eq!(explain_check_error("http status 404 Not Found"), "опубликованных обновлений пока нет");
        assert!(explain_check_error("connection refused").starts_with("не удалось проверить обновления"));
    }

    #[test]
    fn a_package_type_mismatch_points_to_the_releases_page() {
        let msg = explain_install_error("invalid updater binary format");
        assert!(msg.contains(RELEASES_PAGE), "{msg}");
    }

    #[test]
    fn a_bad_signature_is_called_that() {
        assert!(explain_install_error("signature verification failed").contains("подпись"));
    }
}
