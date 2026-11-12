//! A chat turn with the local model: the conversation goes to llama-server's OpenAI-compatible
//! endpoint, the answer comes back as a stream and is sent to the UI piece by piece
//! (`chat-<id>` events: {delta} …, then {done} or {error}). Pictures go as image parts for models
//! that can see.

use crate::{catalog, engine, models, store};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{atomic::AtomicBool, atomic::Ordering, Arc, Mutex},
};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Deserialize, Clone, Debug)]
pub struct Msg {
    pub role: String,
    pub content: String,
    /// data: URLs of pictures attached to this message
    #[serde(default)]
    pub images: Vec<String>,
}

#[derive(Default)]
pub struct ChatState {
    /// running answers: chat id → stop flag
    running: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

/// The request body for /v1/chat/completions. Pictures turn a message into a list of parts.
pub fn request_body(messages: &[Msg], temperature: f32) -> Value {
    let msgs: Vec<Value> = messages
        .iter()
        .filter(|m| matches!(m.role.as_str(), "system" | "user" | "assistant"))
        .map(|m| {
            if m.images.is_empty() {
                json!({ "role": m.role, "content": m.content })
            } else {
                let mut parts = vec![json!({ "type": "text", "text": m.content })];
                parts.extend(m.images.iter().map(|url| json!({ "type": "image_url", "image_url": { "url": url } })));
                json!({ "role": m.role, "content": parts })
            }
        })
        .collect();
    json!({ "messages": msgs, "stream": true, "temperature": temperature, "cache_prompt": true })
}

/// Text pieces of an OpenAI-style SSE stream; `buf` keeps an unfinished line between chunks.
/// Returns the pieces and whether the stream said it is done.
pub fn sse_deltas(buf: &mut String, chunk: &str) -> (Vec<String>, bool) {
    buf.push_str(chunk);
    let mut out = Vec::new();
    let mut done = false;
    while let Some(nl) = buf.find('\n') {
        let line: String = buf.drain(..=nl).collect();
        let line = line.trim();
        let Some(data) = line.strip_prefix("data:") else { continue };
        let data = data.trim();
        if data == "[DONE]" {
            done = true;
            continue;
        }
        if let Ok(v) = serde_json::from_str::<Value>(data) {
            if let Some(s) = v["choices"][0]["delta"]["content"].as_str() {
                if !s.is_empty() {
                    out.push(s.to_string());
                }
            }
        }
    }
    (out, done)
}

/// What a failed request means for the user.
pub fn explain_engine_error(status: u16, body: &str) -> String {
    let b = body.to_ascii_lowercase();
    if b.contains("context") && (b.contains("exceed") || b.contains("too long") || b.contains("size")) {
        return "диалог и вложения не помещаются в память модели — уберите часть вложений или начните новый чат".into();
    }
    if b.contains("image") && (b.contains("not supported") || b.contains("mmproj") || b.contains("multimodal")) {
        return "эта модель не умеет смотреть картинки — выберите модель с пометкой «картинки»".into();
    }
    format!("модель ответила ошибкой ({status})")
}

fn emit(app: &AppHandle, id: &str, payload: Value) {
    let _ = app.emit(&format!("chat-{id}"), payload);
}

/// Sends the conversation to the selected model and streams the answer. Returns when it is complete,
/// stopped or failed (the failure also comes as an {error} event).
#[tauri::command]
pub async fn chat_send(app: AppHandle, id: String, messages: Vec<Msg>, temperature: Option<f32>) -> Result<(), String> {
    if !store::valid_id(&id) {
        return Err("неверный идентификатор чата".into());
    }
    let result = run(&app, &id, &messages, temperature.unwrap_or(0.7)).await;
    app.state::<ChatState>().running.lock().unwrap().remove(&id);
    match result {
        Ok(()) => {
            emit(&app, &id, json!({ "done": true }));
            Ok(())
        }
        Err(e) => {
            emit(&app, &id, json!({ "error": e }));
            Err(e)
        }
    }
}

async fn run(app: &AppHandle, id: &str, messages: &[Msg], temperature: f32) -> Result<(), String> {
    let model_id = engine::settings().selected.ok_or("модель не выбрана — откройте список моделей и установите подходящую")?;
    let model = catalog::catalog().models.iter().find(|m| m.id == model_id).ok_or("выбранной модели нет в каталоге")?;
    if !models::installed(model) {
        return Err("выбранная модель не установлена".into());
    }
    if messages.iter().any(|m| !m.images.is_empty()) && !(model.vision && model.mmproj.is_some()) {
        return Err("эта модель не умеет смотреть картинки".into());
    }
    let stop = Arc::new(AtomicBool::new(false));
    app.state::<ChatState>().running.lock().unwrap().insert(id.to_string(), stop.clone());
    emit(app, id, json!({ "status": "starting" }));
    let (port, token) = engine::ensure(app, &model_id).await?;
    emit(app, id, json!({ "status": "answering" }));
    let client = crate::net::loopback_client()?;
    let mut resp = client
        .post(format!("http://127.0.0.1:{port}/v1/chat/completions"))
        .bearer_auth(&token)
        .json(&request_body(messages, temperature))
        .send()
        .await
        .map_err(|e| format!("нет связи с движком: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        log::warn!("engine answered {status}");
        return Err(explain_engine_error(status, &body));
    }
    let mut buf = String::new();
    loop {
        if stop.load(Ordering::Relaxed) {
            // dropping the response closes the connection: llama-server stops generating
            return Ok(());
        }
        let chunk = match tokio::time::timeout(std::time::Duration::from_millis(500), resp.chunk()).await {
            Err(_) => continue, // nothing yet: check the stop flag again
            Ok(Ok(Some(c))) => c,
            Ok(Ok(None)) => break,
            Ok(Err(e)) => return Err(format!("ответ прервался: {e}")),
        };
        let (pieces, done) = sse_deltas(&mut buf, &String::from_utf8_lossy(&chunk));
        for p in pieces {
            emit(app, id, json!({ "delta": p }));
        }
        engine::touch(app);
        if done {
            break;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn chat_stop(app: AppHandle, id: String) {
    if let Some(s) = app.state::<ChatState>().running.lock().unwrap().get(&id) {
        s.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(role: &str, content: &str) -> Msg {
        Msg { role: role.into(), content: content.into(), images: vec![] }
    }

    #[test]
    fn stream_pieces_survive_split_lines() {
        let mut buf = String::new();
        let (a, done) = sse_deltas(&mut buf, "data: {\"choices\":[{\"delta\":{\"content\":\"При\"}}]}\n\ndata: {\"choi");
        assert_eq!(a, ["При"]);
        assert!(!done);
        let (b, done) = sse_deltas(&mut buf, "ces\":[{\"delta\":{\"content\":\"вет\"}}]}\n\ndata: [DONE]\n\n");
        assert_eq!(b, ["вет"]);
        assert!(done);
    }

    #[test]
    fn role_only_and_empty_deltas_are_skipped() {
        let mut buf = String::new();
        let (a, _) = sse_deltas(&mut buf, "data: {\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}\ndata: {\"choices\":[{\"delta\":{\"content\":\"\"}}]}\n: keep-alive\n");
        assert!(a.is_empty());
    }

    #[test]
    fn pictures_become_image_parts() {
        let mut user = m("user", "Что на фото?");
        user.images.push("data:image/png;base64,AAAA".into());
        let body = request_body(&[m("system", "Ты помощник"), user], 0.7);
        let msgs = body["messages"].as_array().unwrap();
        assert_eq!(msgs[0]["content"], "Ты помощник");
        assert_eq!(msgs[1]["content"][0]["type"], "text");
        assert_eq!(msgs[1]["content"][1]["image_url"]["url"], "data:image/png;base64,AAAA");
        assert_eq!(body["stream"], true);
    }

    #[test]
    fn unknown_roles_never_reach_the_model() {
        let body = request_body(&[m("tool", "x"), m("user", "привет")], 0.7);
        assert_eq!(body["messages"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn engine_errors_are_explained() {
        assert!(explain_engine_error(400, "the request exceeds the available context size").contains("не помещаются"));
        assert!(explain_engine_error(500, "image input is not supported - hint: if this is unexpected, you may need to provide the mmproj").contains("картинки"));
        assert_eq!(explain_engine_error(500, "boom"), "модель ответила ошибкой (500)");
    }
}
