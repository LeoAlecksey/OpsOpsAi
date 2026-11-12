//! Model list for this computer, installation (engine + model + vision projector) in the
//! background with progress events, removal and the choice of the model chats use.

use crate::{
    catalog::{self, Fit, Level},
    download, engine, hw, store,
    store::err,
};
use serde::Serialize;
use serde_json::json;
use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc, Mutex},
};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Default)]
pub struct ModelsState {
    /// running installations: model id → cancel flag
    jobs: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelItem {
    id: String,
    title: String,
    family: String,
    params_b: f32,
    active_b: f32,
    ram_gb: u32,
    license: String,
    description: String,
    vision: bool,
    /// bytes to download (model + vision projector)
    size: u64,
    installed: bool,
    installing: bool,
    fit: Fit,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelList {
    hardware: hw::Hardware,
    selected: Option<String>,
    running: Option<String>,
    /// fits well: shown first
    recommended: Vec<ModelItem>,
    /// runs, but slowly or tight on memory
    possible: Vec<ModelItem>,
    /// models this computer cannot run are not offered; how many were left out
    hidden: usize,
}

pub fn installed(m: &catalog::Model) -> bool {
    engine::model_paths(m).map(|(f, p)| f.exists() && p.is_none_or(|p| p.exists())).unwrap_or(false)
}

/// Sorts the catalog for this computer: recommended first (bigger first — the best that runs well),
/// then possible; models that will not run are left out unless already installed.
pub fn arrange(items: Vec<ModelItem>) -> (Vec<ModelItem>, Vec<ModelItem>, usize) {
    let (mut rec, mut pos, mut hidden) = (Vec::new(), Vec::new(), 0);
    for it in items {
        match it.fit.level {
            Level::Good => rec.push(it),
            Level::Slow | Level::Tight => pos.push(it),
            Level::No if it.installed => pos.push(it),
            Level::No => hidden += 1,
        }
    }
    rec.sort_by(|a, b| b.params_b.total_cmp(&a.params_b));
    pos.sort_by(|a, b| a.params_b.total_cmp(&b.params_b));
    (rec, pos, hidden)
}

#[tauri::command]
pub async fn models_list(app: AppHandle) -> Result<ModelList, String> {
    let data = store::data_dir()?;
    let hardware = tauri::async_runtime::spawn_blocking(move || hw::detect(&data)).await.map_err(err)?;
    let jobs: Vec<String> = app.state::<ModelsState>().jobs.lock().unwrap().keys().cloned().collect();
    let items = catalog::catalog()
        .models
        .iter()
        .map(|m| {
            let inst = installed(m);
            ModelItem {
                id: m.id.clone(),
                title: m.title.clone(),
                family: m.family.clone(),
                params_b: m.params_b,
                active_b: m.active_b,
                ram_gb: m.ram_gb,
                license: m.license.clone(),
                description: m.description.clone(),
                vision: m.vision && m.mmproj.is_some(),
                size: m.download_size(),
                installed: inst,
                installing: jobs.contains(&m.id),
                fit: catalog::fit(m, &hardware, inst),
            }
        })
        .collect();
    let (recommended, possible, hidden) = arrange(items);
    let selected = engine::settings().selected.filter(|id| catalog::catalog().models.iter().any(|m| &m.id == id && installed(m)));
    Ok(ModelList { hardware, selected, running: app.state::<engine::EngineState>().running_model(), recommended, possible, hidden })
}

fn find(id: &str) -> Result<&'static catalog::Model, String> {
    catalog::catalog().models.iter().find(|m| m.id == id).ok_or_else(|| "такой модели нет в каталоге".to_string())
}

/// Installs in the background: the engine (once), the model and its vision projector.
/// Progress: `model-progress` {id, stage, done, total, check}; end: `model-installed` {id} or `model-error` {id, error}.
#[tauri::command]
pub fn model_install(app: AppHandle, id: String) -> Result<(), String> {
    let m = find(&id)?;
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let state = app.state::<ModelsState>();
        let mut jobs = state.jobs.lock().unwrap();
        if jobs.contains_key(&id) {
            return Ok(());
        }
        jobs.insert(id.clone(), cancel.clone());
    }
    tauri::async_runtime::spawn(async move {
        let progress = |stage: &'static str| {
            let (app, id) = (app.clone(), id.clone());
            move |done: u64, total: u64, check: bool| {
                let _ = app.emit("model-progress", json!({ "id": id, "stage": stage, "done": done, "total": total, "check": check }));
            }
        };
        let result: Result<(), String> = async {
            let data = store::data_dir()?;
            let hw = hw::detect(&data);
            let (file, proj) = engine::model_paths(m)?;
            // the engine, the model and its vision projector come at the same time, each in
            // several connections: the slowest file sets the pace, not the sum of all three.
            // Separate tasks, so one file's sha256 pass does not hold up the others' bytes.
            let task = |stage: &'static str, job: Option<(&'static str, PathBuf, &'static str)>| {
                let (cancel, on) = (cancel.clone(), progress(stage));
                tauri::async_runtime::spawn(async move {
                    match job {
                        Some((url, dest, sha)) if !dest.exists() => download::download(url, &dest, sha, &cancel, &on).await,
                        _ => Ok(()),
                    }
                })
            };
            let model = task("model", Some((m.file.url.as_str(), file, m.file.sha256.as_str())));
            let vision = task("vision", m.mmproj.as_ref().zip(proj).map(|(p, path)| (p.url.as_str(), path, p.sha256.as_str())));
            let engine = {
                let (cancel, on, gpu) = (cancel.clone(), progress("engine"), hw.gpu);
                tauri::async_runtime::spawn(async move { engine::install(gpu, &cancel, &on).await.map(|_| ()) })
            };
            let mut first_error = None;
            for (name, handle) in [("engine", engine), ("model", model), ("vision", vision)] {
                let r = handle.await.map_err(|e| format!("{name}: {e}")).and_then(|r| r);
                if let Err(e) = r {
                    // one file failed: stop the others (their .part stays for a resume)
                    cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                    first_error.get_or_insert(e);
                }
            }
            if let Some(e) = first_error {
                return Err(e);
            }
            // the first installed model becomes the one chats use
            let mut s = engine::settings();
            if s.selected.as_ref().is_none_or(|sel| find(sel).map(|m| !installed(m)).unwrap_or(true)) {
                s.selected = Some(m.id.clone());
                engine::save_settings(&s)?;
            }
            Ok(())
        }
        .await;
        app.state::<ModelsState>().jobs.lock().unwrap().remove(&id);
        match result {
            Ok(()) => {
                log::info!("model {id} installed");
                let _ = app.emit("model-installed", json!({ "id": id }));
            }
            Err(e) => {
                log::warn!("model {id}: {e}");
                let _ = app.emit("model-error", json!({ "id": id, "error": e }));
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub fn model_cancel(app: AppHandle, id: String) {
    if let Some(c) = app.state::<ModelsState>().jobs.lock().unwrap().get(&id) {
        c.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

#[tauri::command]
pub async fn model_remove(app: AppHandle, id: String) -> Result<(), String> {
    let m = find(&id)?;
    let state = app.state::<engine::EngineState>();
    if state.running_model().as_deref() == Some(id.as_str()) {
        state.stop();
    }
    let (file, _) = engine::model_paths(m)?;
    if let Some(dir) = file.parent() {
        if dir.exists() {
            let dir = dir.to_path_buf();
            tauri::async_runtime::spawn_blocking(move || fs::remove_dir_all(dir)).await.map_err(err)?.map_err(err)?;
        }
    }
    let mut s = engine::settings();
    if s.selected.as_deref() == Some(id.as_str()) {
        // another installed model takes its place, if there is one
        s.selected = catalog::catalog().models.iter().find(|m| m.id != id && installed(m)).map(|m| m.id.clone());
        engine::save_settings(&s)?;
    }
    log::info!("model {id} removed");
    Ok(())
}

#[tauri::command]
pub fn model_select(id: String) -> Result<(), String> {
    let m = find(&id)?;
    if !installed(m) {
        return Err("сначала установите эту модель".into());
    }
    let mut s = engine::settings();
    s.selected = Some(id);
    engine::save_settings(&s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, params: f32, level: Level, installed: bool) -> ModelItem {
        ModelItem {
            id: id.into(),
            title: id.into(),
            family: String::new(),
            params_b: params,
            active_b: params,
            ram_gb: 8,
            license: String::new(),
            description: String::new(),
            vision: true,
            size: 1,
            installed,
            installing: false,
            fit: Fit { level, why: String::new() },
        }
    }

    #[test]
    fn models_that_will_not_run_are_not_offered() {
        let (rec, pos, hidden) = arrange(vec![
            item("small", 2.0, Level::Good, false),
            item("mid", 4.0, Level::Good, false),
            item("tight", 9.0, Level::Tight, false),
            item("huge", 27.0, Level::No, false),
        ]);
        assert_eq!(rec.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), ["mid", "small"], "лучшая из подходящих — первой");
        assert_eq!(pos.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), ["tight"]);
        assert_eq!(hidden, 1);
    }

    #[test]
    fn an_installed_model_stays_visible_so_it_can_be_removed() {
        let (_, pos, hidden) = arrange(vec![item("huge", 27.0, Level::No, true)]);
        assert_eq!(pos.len(), 1);
        assert_eq!(hidden, 0);
    }
}
