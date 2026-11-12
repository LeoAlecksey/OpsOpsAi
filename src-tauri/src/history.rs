//! Chat history: one JSON file per chat in the data folder, written atomically. Stays on this
//! computer; nothing is sent anywhere.

use crate::{store, store::err};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

/// A chat larger than this is not saved whole: very long chats are trimmed from the start.
const MAX_MESSAGES: usize = 500;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct StoredMsg {
    pub role: String,
    pub content: String,
    /// data: URLs of attached pictures
    pub images: Vec<String>,
    /// names of attached files and folders (their text is part of `content`)
    pub attachments: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Chat {
    pub id: String,
    pub title: String,
    /// unix ms
    pub created: i64,
    pub updated: i64,
    pub model: Option<String>,
    pub skill: Option<String>,
    pub messages: Vec<StoredMsg>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ChatMeta {
    pub id: String,
    pub title: String,
    pub updated: i64,
}

fn chats_dir() -> Result<PathBuf, String> {
    let d = store::data_dir()?.join("chats");
    fs::create_dir_all(&d).map_err(err)?;
    Ok(d)
}

fn file_name(id: &str) -> Result<String, String> {
    if !store::valid_id(id) {
        return Err("неверный идентификатор чата".into());
    }
    Ok(format!("{id}.json"))
}

/// Chats newest first, by their last change.
pub fn sort_meta(mut list: Vec<ChatMeta>) -> Vec<ChatMeta> {
    list.sort_by_key(|c| std::cmp::Reverse(c.updated));
    list
}

#[tauri::command]
pub async fn chats_list() -> Result<Vec<ChatMeta>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let dir = chats_dir()?;
        let mut out = Vec::new();
        for e in fs::read_dir(&dir).map_err(err)?.flatten() {
            let p = e.path();
            if p.extension().is_none_or(|x| x != "json") {
                continue;
            }
            // a broken file must not hide the other chats
            if let Ok(c) = serde_json::from_str::<Chat>(&fs::read_to_string(&p).unwrap_or_default()) {
                if store::valid_id(&c.id) {
                    out.push(ChatMeta { id: c.id, title: c.title, updated: c.updated });
                }
            }
        }
        Ok(sort_meta(out))
    })
    .await
    .map_err(err)?
}

#[tauri::command]
pub async fn chat_load(id: String) -> Result<Chat, String> {
    let name = file_name(&id)?;
    tauri::async_runtime::spawn_blocking(move || store::load_json_in::<Chat>(&chats_dir()?, &name)).await.map_err(err)?
}

#[tauri::command]
pub async fn chat_save(mut chat: Chat) -> Result<(), String> {
    let name = file_name(&chat.id)?;
    if chat.messages.len() > MAX_MESSAGES {
        let cut = chat.messages.len() - MAX_MESSAGES;
        chat.messages.drain(..cut);
    }
    tauri::async_runtime::spawn_blocking(move || store::write_json_in(&chats_dir()?, &name, &serde_json::to_string(&chat).map_err(err)?))
        .await
        .map_err(err)?
}

#[tauri::command]
pub async fn chat_delete(id: String) -> Result<(), String> {
    let name = file_name(&id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let dir = chats_dir()?;
        for n in [name.clone(), format!("{name}.bak")] {
            let p = dir.join(n);
            if p.exists() {
                fs::remove_file(p).map_err(err)?;
            }
        }
        Ok(())
    })
    .await
    .map_err(err)?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newest_chat_comes_first() {
        let l = sort_meta(vec![
            ChatMeta { id: "a".into(), title: "old".into(), updated: 1 },
            ChatMeta { id: "b".into(), title: "new".into(), updated: 3 },
            ChatMeta { id: "c".into(), title: "mid".into(), updated: 2 },
        ]);
        assert_eq!(l.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["b", "c", "a"]);
    }

    #[test]
    fn ids_that_could_escape_the_folder_are_refused() {
        assert!(file_name("../secret").is_err());
        assert!(file_name("a/b").is_err());
        assert_eq!(file_name("3f2b-9c1e").unwrap(), "3f2b-9c1e.json");
    }

    #[test]
    fn a_chat_round_trips_and_disappears_on_delete() {
        let id = "test-round-trip".to_string();
        let chat = Chat {
            id: id.clone(),
            title: "Тест".into(),
            created: 1,
            updated: 2,
            messages: vec![StoredMsg { role: "user".into(), content: "привет".into(), ..Default::default() }],
            ..Default::default()
        };
        tauri::async_runtime::block_on(chat_save(chat)).unwrap();
        let back = tauri::async_runtime::block_on(chat_load(id.clone())).unwrap();
        assert_eq!(back.messages[0].content, "привет");
        assert!(tauri::async_runtime::block_on(chats_list()).unwrap().iter().any(|c| c.id == id));
        tauri::async_runtime::block_on(chat_delete(id.clone())).unwrap();
        assert!(!tauri::async_runtime::block_on(chats_list()).unwrap().iter().any(|c| c.id == id));
    }
}
