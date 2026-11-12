//! Shared persistence: JSON files in the app's config folder, written atomically with a backup copy.
//!
//! Origin: adapted from OpsDeck `src-tauri/src/store.rs` (fragments by the same author only).
//! Keyring helpers come back with the first secret that needs them.

use serde::{de::DeserializeOwned, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Folder name under the OS config dir (~/.config on Linux, ~/Library/Application Support on
/// macOS, %APPDATA% on Windows).
const APP_DIR: &str = "opsopsai";

pub fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// The app's settings folder, created on first use.
pub fn config_dir() -> Result<PathBuf, String> {
    // unit tests must never read or rewrite the developer's real settings
    #[cfg(test)]
    let root = std::env::temp_dir().join(format!("opsopsai-test-{}", std::process::id()));
    #[cfg(not(test))]
    let root = dirs::config_dir().ok_or("не найдена папка настроек пользователя")?;
    let dir = root.join(APP_DIR);
    fs::create_dir_all(&dir).map_err(err)?;
    Ok(dir)
}

/// The app's data folder (engine, models, chats), created on first use. Models are big, so this
/// is the local (non-roaming) data folder on Windows.
pub fn data_dir() -> Result<PathBuf, String> {
    #[cfg(test)]
    let root = std::env::temp_dir().join(format!("opsopsai-test-data-{}", std::process::id()));
    #[cfg(not(test))]
    let root = dirs::data_local_dir().ok_or("не найдена папка данных пользователя")?;
    let dir = root.join(APP_DIR);
    fs::create_dir_all(&dir).map_err(err)?;
    Ok(dir)
}

/// Previous copy of a config file: save_json renames the old file here before putting the new one in
/// place, so an interrupted write can never leave the app without its settings.
fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("config");
    path.with_file_name(format!("{name}.{suffix}"))
}

pub fn load_json<T: DeserializeOwned + Default>(name: &str) -> Result<T, String> {
    load_json_in(&config_dir()?, name)
}

pub fn load_json_in<T: DeserializeOwned + Default>(dir: &Path, name: &str) -> Result<T, String> {
    let path = dir.join(name);
    let bak = sibling(&path, "bak");
    let parsed = |p: &Path| -> Option<Result<T, String>> {
        let raw = fs::read_to_string(p).ok()?;
        Some(serde_json::from_str(&raw).map_err(|e| format!("{}: {e}", p.display())))
    };
    match parsed(&path) {
        Some(Ok(v)) => return Ok(v),
        // the file is there but unreadable: fall back to the previous copy before anything overwrites it
        Some(Err(e)) => {
            if let Some(Ok(v)) = parsed(&bak) {
                log::warn!("{e}; взята предыдущая копия {}", bak.display());
                return Ok(v);
            }
            let broken = sibling(&path, "bad");
            let _ = fs::copy(&path, &broken);
            return Err(format!("{e} (копия сохранена: {})", broken.display()));
        }
        // killed between the two renames of save_json: only the previous copy is left
        None => {
            if let Some(Ok(v)) = parsed(&bak) {
                log::warn!("{} не найден, взята предыдущая копия {}", path.display(), bak.display());
                return Ok(v);
            }
        }
    }
    Ok(T::default())
}

/// Writes atomically and keeps the previous version next to it: `fs::write` truncates first, so an
/// app killed mid-write (an update, a reboot) would leave an empty file.
pub fn save_json<T: Serialize>(name: &str, value: &T) -> Result<(), String> {
    write_json_in(&config_dir()?, name, &serde_json::to_string_pretty(value).map_err(err)?)
}

pub fn write_json_in(dir: &Path, name: &str, data: &str) -> Result<(), String> {
    let path = dir.join(name);
    let tmp = sibling(&path, "tmp");
    {
        use std::io::Write;
        let mut f = fs::File::create(&tmp).map_err(err)?;
        f.write_all(data.as_bytes()).map_err(err)?;
        f.sync_all().map_err(err)?;
    }
    restrict(&tmp, 0o600)?;
    if path.exists() {
        let _ = fs::rename(&path, sibling(&path, "bak"));
    }
    fs::rename(&tmp, &path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        err(e)
    })?;
    restrict(&path, 0o600)
}

/// chmod on Unix (private files: 0600, dirs: 0700). On Windows files in the user profile
/// are already private to the user, so this is a no-op there.
pub fn restrict(path: &Path, mode: u32) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(err)
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
        Ok(())
    }
}

/// Opens a folder or URL with the system handler, without a shell.
pub fn open_with_system(target: &str) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    let mut cmd = std::process::Command::new("xdg-open");
    #[cfg(target_os = "macos")]
    let mut cmd = std::process::Command::new("open");
    #[cfg(target_os = "windows")]
    let mut cmd = {
        // rundll32 avoids cmd.exe quoting problems with & in URLs
        let mut c = std::process::Command::new("rundll32");
        c.arg("url.dll,FileProtocolHandler");
        c
    };
    let mut child = cmd.arg(target).spawn().map_err(err)?;
    std::thread::spawn(move || child.wait());
    Ok(())
}

/// Constant-time comparison for tokens (no early exit that would leak the matching prefix).
pub fn ct_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Ids come from the frontend (crypto.randomUUID) and end up in file names and event names.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Serialize, Deserialize, Default, Debug, PartialEq)]
    struct Conf {
        name: String,
    }

    fn fresh_dir(tag: &str) -> PathBuf {
        let d = config_dir().unwrap().join(tag);
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn write(dir: &Path, name: &str, c: &Conf) {
        write_json_in(dir, name, &serde_json::to_string(c).unwrap()).unwrap();
    }

    #[test]
    fn a_missing_file_gives_the_defaults() {
        let d = fresh_dir("missing");
        assert_eq!(load_json_in::<Conf>(&d, "c.json").unwrap(), Conf::default());
    }

    #[test]
    fn a_save_keeps_the_previous_copy() {
        let d = fresh_dir("backup");
        write(&d, "c.json", &Conf { name: "one".into() });
        write(&d, "c.json", &Conf { name: "two".into() });
        assert_eq!(load_json_in::<Conf>(&d, "c.json").unwrap().name, "two");
        let bak: Conf = serde_json::from_str(&fs::read_to_string(d.join("c.json.bak")).unwrap()).unwrap();
        assert_eq!(bak.name, "one");
        assert!(!d.join("c.json.tmp").exists());
    }

    #[test]
    fn a_broken_file_falls_back_to_the_previous_copy() {
        let d = fresh_dir("broken");
        write(&d, "c.json", &Conf { name: "good".into() });
        write(&d, "c.json", &Conf { name: "newer".into() });
        fs::write(d.join("c.json"), "{ not json").unwrap();
        assert_eq!(load_json_in::<Conf>(&d, "c.json").unwrap().name, "good");
    }

    #[test]
    fn a_broken_file_without_a_copy_is_an_error_and_is_kept() {
        let d = fresh_dir("broken-alone");
        fs::write(d.join("c.json"), "{ not json").unwrap();
        assert!(load_json_in::<Conf>(&d, "c.json").is_err());
        assert!(d.join("c.json.bad").exists());
    }

    #[test]
    fn only_the_previous_copy_is_left() {
        let d = fresh_dir("only-bak");
        fs::write(d.join("c.json.bak"), r#"{"name":"saved"}"#).unwrap();
        assert_eq!(load_json_in::<Conf>(&d, "c.json").unwrap().name, "saved");
    }

    #[cfg(unix)]
    #[test]
    fn saved_files_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let d = fresh_dir("perm");
        write(&d, "c.json", &Conf::default());
        assert_eq!(fs::metadata(d.join("c.json")).unwrap().permissions().mode() & 0o777, 0o600);
    }

    #[test]
    fn ids() {
        assert!(valid_id("3f2b9c1e-7a4d-4e8a-9b1c-0d2e3f4a5b6c"));
        for bad in ["", "../x", "a b", "a/b", &"a".repeat(65)] {
            assert!(!valid_id(bad), "{bad}");
        }
    }

    #[test]
    fn token_compare() {
        assert!(ct_eq("secret", "secret"));
        assert!(!ct_eq("secret", "secreT"));
        assert!(!ct_eq("secret", "secret1"));
        assert!(ct_eq("", ""));
    }
}
