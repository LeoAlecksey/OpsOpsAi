//! Attachments: text files and folders become text for the model's context, pictures become
//! data: URLs for models that can see. The user picks them in the system dialog; nothing is read
//! without that.

use crate::store::err;
use base64::Engine as _;
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
};
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

/// Text taken from one attachment at most (characters): more would not fit the model's memory anyway.
const MAX_TEXT: usize = 60_000;
/// Files read from one folder at most.
const MAX_FILES: usize = 300;
const MAX_DEPTH: usize = 6;
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_IMAGE_BYTES: u64 = 8 * 1024 * 1024;

const SKIP_DIRS: &[&str] = &[".git", "node_modules", "target", ".venv", "venv", "__pycache__", "dist", "build", ".idea", ".vscode"];

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub name: String,
    /// "text" or "image"
    pub kind: String,
    /// the text, or a data: URL for a picture
    pub content: String,
    /// files read (1 for a single file)
    pub files: usize,
    /// something was left out: too long, too many files, or files that are not text
    pub truncated: bool,
    /// why nothing could be taken, when content is empty
    pub problem: Option<String>,
}

fn image_mime(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        _ => None,
    }
}

/// Text in a file: valid UTF-8 without NUL bytes (binary files are left out).
fn read_text(path: &Path) -> Option<String> {
    if fs::metadata(path).ok()?.len() > MAX_FILE_BYTES {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    if bytes.contains(&0) {
        return None;
    }
    String::from_utf8(bytes).ok()
}

fn display_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_string_lossy().into_owned())
}

/// Appends `text` within the budget; false when the budget ran out.
fn push_limited(out: &mut String, text: &str) -> bool {
    let room = MAX_TEXT.saturating_sub(out.chars().count());
    if room == 0 {
        return false;
    }
    if text.chars().count() <= room {
        out.push_str(text);
        true
    } else {
        out.extend(text.chars().take(room));
        false
    }
}

fn walk(dir: &Path, depth: usize, files: &mut Vec<PathBuf>) {
    if depth > MAX_DEPTH || files.len() >= MAX_FILES {
        return;
    }
    let Ok(rd) = fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        let name = display_name(&p);
        if name.starts_with('.') {
            continue;
        }
        if p.is_dir() {
            if !SKIP_DIRS.contains(&name.as_str()) {
                walk(&p, depth + 1, files);
            }
        } else if files.len() < MAX_FILES {
            files.push(p);
        }
    }
}

/// Reads what the user picked. Pure enough to be tested on any OS.
pub fn read_path(path: &Path) -> Attachment {
    let name = display_name(path);
    let empty = |problem: &str| Attachment { name: name.clone(), kind: "text".into(), content: String::new(), files: 0, truncated: false, problem: Some(problem.into()) };
    if path.is_dir() {
        let mut files = Vec::new();
        walk(path, 0, &mut files);
        let mut text = String::new();
        let (mut read, mut skipped, mut full) = (0, false, false);
        for f in &files {
            let Some(body) = read_text(f) else {
                skipped = true;
                continue;
            };
            let rel = f.strip_prefix(path).unwrap_or(f).to_string_lossy().replace('\\', "/");
            if !push_limited(&mut text, &format!("--- {rel} ---\n{body}\n\n")) {
                full = true;
                read += 1;
                break;
            }
            read += 1;
        }
        if read == 0 {
            return empty("в папке нет текстовых файлов");
        }
        return Attachment { name, kind: "text".into(), content: text, files: read, truncated: skipped || full || files.len() >= MAX_FILES, problem: None };
    }
    if let Some(mime) = image_mime(path) {
        return match fs::metadata(path) {
            Ok(m) if m.len() > MAX_IMAGE_BYTES => empty("картинка больше 8 МБ"),
            Ok(_) => match fs::read(path) {
                Ok(b) => Attachment {
                    name,
                    kind: "image".into(),
                    content: format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(b)),
                    files: 1,
                    truncated: false,
                    problem: None,
                },
                Err(e) => empty(&format!("не удалось прочитать: {e}")),
            },
            Err(e) => empty(&format!("не удалось прочитать: {e}")),
        };
    }
    match read_text(path) {
        Some(body) => {
            let mut text = String::new();
            let whole = push_limited(&mut text, &body);
            Attachment { name, kind: "text".into(), content: text, files: 1, truncated: !whole, problem: None }
        }
        None => empty("это не текстовый файл (PDF и документы Word пока не поддерживаются)"),
    }
}

/// Opens the system picker and reads what was chosen. `folder` — pick folders instead of files.
#[tauri::command]
pub async fn attach_pick(app: AppHandle, folder: bool) -> Result<Vec<Attachment>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let d = app.dialog().file();
        let picked: Vec<PathBuf> = if folder {
            d.blocking_pick_folders().unwrap_or_default().into_iter().filter_map(|p| p.into_path().ok()).collect()
        } else {
            d.blocking_pick_files().unwrap_or_default().into_iter().filter_map(|p| p.into_path().ok()).collect()
        };
        picked.iter().map(|p| read_path(p)).collect()
    })
    .await
    .map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("opsopsai-attach-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_text_file_is_read_whole() {
        let d = dir("file");
        fs::write(d.join("note.md"), "# Заметка\nтекст").unwrap();
        let a = read_path(&d.join("note.md"));
        assert_eq!((a.kind.as_str(), a.content.as_str(), a.files, a.truncated), ("text", "# Заметка\nтекст", 1, false));
    }

    #[test]
    fn a_folder_takes_text_files_and_skips_binaries_and_service_dirs() {
        let d = dir("folder");
        fs::write(d.join("a.txt"), "первый").unwrap();
        fs::create_dir_all(d.join("sub")).unwrap();
        fs::write(d.join("sub/b.rs"), "fn main() {}").unwrap();
        fs::write(d.join("blob.bin"), [0u8, 1, 2, 3]).unwrap();
        fs::create_dir_all(d.join("node_modules/x")).unwrap();
        fs::write(d.join("node_modules/x/c.js"), "skip").unwrap();
        fs::write(d.join(".hidden"), "skip").unwrap();
        let a = read_path(&d);
        assert_eq!(a.files, 2);
        assert!(a.content.contains("--- a.txt ---\nпервый"));
        assert!(a.content.contains("--- sub/b.rs ---"));
        assert!(!a.content.contains("skip"));
        assert!(a.truncated, "бинарный файл пропущен — об этом сказано");
    }

    #[test]
    fn long_text_is_cut_to_the_budget() {
        let d = dir("long");
        fs::write(d.join("big.txt"), "я".repeat(MAX_TEXT + 100)).unwrap();
        let a = read_path(&d.join("big.txt"));
        assert_eq!(a.content.chars().count(), MAX_TEXT);
        assert!(a.truncated);
    }

    #[test]
    fn a_picture_becomes_a_data_url() {
        let d = dir("img");
        fs::write(d.join("p.PNG"), [0x89, b'P', b'N', b'G']).unwrap();
        let a = read_path(&d.join("p.PNG"));
        assert_eq!(a.kind, "image");
        assert!(a.content.starts_with("data:image/png;base64,"));
    }

    #[test]
    fn a_binary_file_is_explained_not_sent() {
        let d = dir("bin");
        fs::write(d.join("doc.pdf"), [0x25, 0x50, 0x44, 0x46, 0, 1]).unwrap();
        let a = read_path(&d.join("doc.pdf"));
        assert!(a.content.is_empty());
        assert!(a.problem.unwrap().contains("PDF"));
    }
}
