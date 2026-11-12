//! The local inference engine: llama.cpp's `llama-server`, a pinned build downloaded on demand
//! (sha256 from catalog.json, never trusted from the network), started on 127.0.0.1 with a session
//! token for the chosen model, moved to the CPU when the GPU fails, stopped after a while unused.
//!
//! Launch and GPU fallback are adapted from OpsDeck `src-tauri/src/ai.rs` (same author).

use crate::{catalog, download, store, store::err};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager};

/// Stop the engine (and free the model's memory) after this long without requests.
const IDLE_STOP: Duration = Duration::from_secs(15 * 60);

/// Engine settings kept in ai.json next to the other settings.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct AiSettings {
    /// the model chats use; None until one is installed
    pub selected: Option<String>,
    /// use the GPU when possible
    pub gpu: bool,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self { selected: None, gpu: true }
    }
}

pub fn settings() -> AiSettings {
    store::load_json("ai.json").unwrap_or_default()
}

pub fn save_settings(s: &AiSettings) -> Result<(), String> {
    store::save_json("ai.json", s)
}

/// The engine archive for this OS/CPU: the Vulkan build (any NVIDIA/AMD/Intel GPU) or the CPU one;
/// macOS builds always include Metal.
pub fn asset_suffix(os: &str, arch: &str, gpu: bool) -> Option<&'static str> {
    Some(match (os, arch, gpu) {
        ("linux", "x86_64", false) => "bin-ubuntu-x64.tar.gz",
        ("linux", "x86_64", true) => "bin-ubuntu-vulkan-x64.tar.gz",
        ("linux", "aarch64", false) => "bin-ubuntu-arm64.tar.gz",
        ("linux", "aarch64", true) => "bin-ubuntu-vulkan-arm64.tar.gz",
        ("macos", "aarch64", _) => "bin-macos-arm64.tar.gz",
        ("windows", "x86_64", false) => "bin-win-cpu-x64.zip",
        ("windows", "x86_64", true) => "bin-win-vulkan-x64.zip",
        _ => return None,
    })
}

/// Which build to use here: the GPU one when the computer has a GPU the engine can use.
fn want_gpu_build(hw_gpu: bool) -> bool {
    hw_gpu && !cfg!(target_os = "macos")
}

fn engine_root() -> Result<PathBuf, String> {
    Ok(store::data_dir()?.join("engine"))
}

fn engine_dir(gpu_build: bool) -> Result<PathBuf, String> {
    let tag = &catalog::catalog().engine.tag;
    Ok(engine_root()?.join(format!("llama-{tag}{}", if gpu_build { "-vulkan" } else { "" })))
}

fn server_name() -> &'static str {
    if cfg!(windows) {
        "llama-server.exe"
    } else {
        "llama-server"
    }
}

/// llama-server somewhere inside the unpacked engine folder.
fn find_server(dir: &Path) -> Option<PathBuf> {
    for e in fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        if p.is_dir() {
            if let Some(f) = find_server(&p) {
                return Some(f);
            }
        } else if p.file_name().is_some_and(|n| n == server_name()) {
            return Some(p);
        }
    }
    None
}

/// The installed engine binary, preferring the GPU build when wanted.
pub fn installed_binary(gpu_build: bool) -> Option<PathBuf> {
    let gpu = if gpu_build { engine_dir(true).ok().and_then(|d| find_server(&d)) } else { None };
    gpu.or_else(|| engine_dir(false).ok().and_then(|d| find_server(&d)))
}

fn unpack(archive: &Path, into: &Path) -> Result<(), String> {
    let _ = fs::remove_dir_all(into);
    fs::create_dir_all(into).map_err(err)?;
    if archive.to_string_lossy().ends_with(".zip") {
        let mut z = zip::ZipArchive::new(fs::File::open(archive).map_err(err)?).map_err(err)?;
        z.extract(into).map_err(err)?;
    } else {
        let gz = flate2::read::GzDecoder::new(fs::File::open(archive).map_err(err)?);
        let mut t = tar::Archive::new(gz);
        t.set_preserve_permissions(true);
        t.unpack(into).map_err(err)?;
    }
    #[cfg(unix)]
    if let Some(bin) = find_server(into) {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&bin, fs::Permissions::from_mode(0o755));
    }
    Ok(())
}

/// Downloads and unpacks the engine if it is not there yet. The sha256 comes from the catalog
/// compiled into the app: a different archive is rejected, whatever the network says.
pub async fn install(hw_gpu: bool, cancel: &AtomicBool, progress: &dyn download::Progress) -> Result<PathBuf, String> {
    let gpu_build = want_gpu_build(hw_gpu);
    if let Some(bin) = engine_dir(gpu_build).ok().and_then(|d| find_server(&d)) {
        return Ok(bin);
    }
    let cat = catalog::catalog();
    let suffix = asset_suffix(std::env::consts::OS, std::env::consts::ARCH, gpu_build)
        .ok_or_else(|| format!("локальный ИИ пока не поддерживается на {}/{}", std::env::consts::OS, std::env::consts::ARCH))?;
    let name = format!("llama-{}-{suffix}", cat.engine.tag);
    let asset = cat.engine.assets.get(&name).ok_or_else(|| format!("в каталоге нет движка {name}"))?;
    let url = format!("https://github.com/ggml-org/llama.cpp/releases/download/{}/{name}", cat.engine.tag);
    let archive = engine_root()?.join(&name);
    download::download(&url, &archive, &asset.sha256, cancel, progress).await?;
    let into = engine_dir(gpu_build)?;
    let a = archive.clone();
    let i = into.clone();
    tauri::async_runtime::spawn_blocking(move || unpack(&a, &i)).await.map_err(err)??;
    let _ = fs::remove_file(&archive);
    find_server(&into).ok_or_else(|| "в архиве движка не найден llama-server".to_string())
}

struct Server {
    child: Child,
    port: u16,
    token: String,
    model: String,
    last_used: Instant,
}

#[derive(Default)]
pub struct EngineState {
    server: Mutex<Option<Server>>,
    /// the GPU did not start this session: stay on the CPU until the app restarts
    gpu_failed: AtomicBool,
    /// one start at a time: two chats asking at once must not start two engines
    starting: tokio::sync::Mutex<()>,
}

impl Drop for EngineState {
    fn drop(&mut self) {
        self.stop();
    }
}

impl EngineState {
    pub fn stop(&self) {
        if let Some(mut s) = self.server.lock().unwrap().take() {
            let _ = s.child.kill();
            let _ = s.child.wait();
            log::info!("engine stopped");
        }
    }

    /// The running model, if any.
    pub fn running_model(&self) -> Option<String> {
        self.server.lock().unwrap().as_ref().map(|s| s.model.clone())
    }
}

fn free_port() -> Result<u16, String> {
    let l = std::net::TcpListener::bind("127.0.0.1:0").map_err(err)?;
    Ok(l.local_addr().map_err(err)?.port())
}

/// A random token for this engine session: other programs on the computer cannot use the model.
fn new_token() -> String {
    format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Device {
    /// all layers that fit into video memory (Metal on macOS)
    Gpu,
    /// CPU only, even with a GPU build of the engine
    Cpu,
}

/// Context window, tokens: longer chats and attachments need more, but take memory.
pub fn ctx_size(ram_total: u64) -> u32 {
    if ram_total >= 15 * (1 << 30) {
        16384
    } else {
        8192
    }
}

/// Arguments for llama-server (without the program). Pure, so it is tested on any OS.
pub fn launch_args(model: &Path, mmproj: Option<&Path>, port: u16, token: &str, ctx: u32, threads: usize, device: Device) -> Vec<String> {
    let mut a: Vec<String> = vec!["-m".into(), model.to_string_lossy().into_owned()];
    if let Some(p) = mmproj {
        a.extend(["--mmproj".into(), p.to_string_lossy().into_owned()]);
    }
    a.extend(
        [
            "--host", "127.0.0.1", "--port", &port.to_string(), "--api-key", token, "-c", &ctx.to_string(), "-t", &threads.to_string(),
            // answers right away instead of a long hidden "thinking" pass
            "--reasoning-budget", "0",
        ]
        .map(String::from),
    );
    match device {
        Device::Gpu => a.extend(["-ngl".into(), "auto".into()]),
        Device::Cpu => a.extend(["-ngl".into(), "0".into(), "--device".into(), "none".into()]),
    }
    a
}

fn no_console(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = cmd;
}

/// Starts llama-server and waits until the model is loaded.
async fn launch(bin: &Path, model: &Path, mmproj: Option<&Path>, ctx: u32, device: Device) -> Result<(Child, u16, String), String> {
    // big models take a while to read from disk: ~15 s per GB on top of the base 90 s
    let size = fs::metadata(model).map(|m| m.len()).unwrap_or(0);
    let limit = Duration::from_secs(90 + size / (1 << 30) * 15);
    let port = free_port()?;
    let token = new_token();
    let threads = std::thread::available_parallelism().map(|n| n.get().clamp(2, 8)).unwrap_or(4);
    // the engine's own output goes to a file: when it fails to start, its last lines say why
    let log_path = engine_root()?.join("server.log");
    let log_file = fs::File::create(&log_path).map_err(err)?;
    let mut cmd = Command::new(bin);
    cmd.args(launch_args(model, mmproj, port, &token, ctx, threads, device))
        .current_dir(bin.parent().unwrap_or(Path::new(".")))
        .stdin(Stdio::null())
        .stdout(log_file.try_clone().map_err(err)?)
        .stderr(log_file);
    no_console(&mut cmd);
    let mut child = cmd.spawn().map_err(|e| format!("не удалось запустить движок: {e}"))?;
    let client = crate::net::loopback_client()?;
    let started = Instant::now();
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            let tail = last_lines(&fs::read_to_string(&log_path).unwrap_or_default(), 12);
            log::warn!("engine exited at start ({status}):\n{tail}");
            return Err(format!("движок завершился при запуске ({status}){}", reason(&tail)));
        }
        if let Ok(r) = client.get(format!("http://127.0.0.1:{port}/health")).send().await {
            if r.status().is_success() {
                return Ok((child, port, token));
            }
        }
        if started.elapsed() > limit {
            let _ = child.kill();
            return Err("модель не успела загрузиться — возможно, не хватает памяти".into());
        }
        tokio::time::sleep(Duration::from_millis(400)).await;
    }
}

/// The last `n` non-empty lines of the engine's output.
fn last_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

/// The line of the engine's output that says what went wrong, for the user.
fn reason(tail: &str) -> String {
    tail.lines()
        .rev()
        .find(|l| {
            let l = l.to_ascii_lowercase();
            l.contains("error") || l.contains("failed") || l.contains("not found") || l.contains("glibc") || l.contains("illegal")
        })
        .map(|l| format!(": {}", l.trim().chars().take(200).collect::<String>()))
        .unwrap_or_default()
}

/// Local paths of a catalog model's files.
pub fn model_paths(m: &catalog::Model) -> Result<(PathBuf, Option<PathBuf>), String> {
    let dir = store::data_dir()?.join("models").join(&m.id);
    Ok((dir.join(&m.file.name), m.mmproj.as_ref().map(|p| dir.join(&p.name))))
}

/// The engine running `model_id`: the address port and the token. Starts or restarts it as needed.
pub async fn ensure(app: &AppHandle, model_id: &str) -> Result<(u16, String), String> {
    let state = app.state::<EngineState>();
    let _one = state.starting.lock().await;
    {
        let mut g = state.server.lock().unwrap();
        if let Some(s) = g.as_mut() {
            if s.model == model_id && s.child.try_wait().ok().flatten().is_none() {
                s.last_used = Instant::now();
                return Ok((s.port, s.token.clone()));
            }
        }
    }
    state.stop();
    let m = catalog::catalog().models.iter().find(|m| m.id == model_id).ok_or("такой модели нет в каталоге")?;
    let (model, mmproj) = model_paths(m)?;
    if !model.exists() {
        return Err("модель не скачана — выберите и установите её в списке моделей".into());
    }
    let mmproj = mmproj.filter(|p| p.exists());
    let hw = crate::hw::detect(&store::data_dir()?);
    let gpu = settings().gpu && hw.gpu && !state.gpu_failed.load(Ordering::Relaxed);
    let bin = installed_binary(want_gpu_build(hw.gpu)).ok_or("движок не установлен — установите модель ещё раз")?;
    let ctx = ctx_size(hw.ram_total);
    let first = launch(&bin, &model, mmproj.as_deref(), ctx, if gpu { Device::Gpu } else { Device::Cpu }).await;
    let (child, port, token) = match first {
        Ok(v) => v,
        Err(e) if gpu => {
            // the GPU did not work out (no driver, too little video memory): run on the CPU instead
            log::warn!("engine: GPU start failed ({e}), falling back to CPU");
            state.gpu_failed.store(true, Ordering::Relaxed);
            launch(&bin, &model, mmproj.as_deref(), ctx, Device::Cpu).await.map_err(|e| format!("модель не запустилась ни на видеокарте, ни на процессоре: {e}"))?
        }
        Err(e) => return Err(e),
    };
    log::info!("engine: {model_id} on 127.0.0.1:{port} ({}, ctx {ctx})", if gpu && !state.gpu_failed.load(Ordering::Relaxed) { "gpu" } else { "cpu" });
    *state.server.lock().unwrap() = Some(Server { child, port, token: token.clone(), model: model_id.to_string(), last_used: Instant::now() });
    Ok((port, token))
}

/// The engine was just used: it is unloaded only after IDLE_STOP without use.
pub fn touch(app: &AppHandle) {
    if let Some(s) = app.state::<EngineState>().server.lock().unwrap().as_mut() {
        s.last_used = Instant::now();
    }
}

/// Background: stop the engine after IDLE_STOP without requests, so the model's memory comes back.
pub fn spawn_idle_stop(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            let state = app.state::<EngineState>();
            let idle = state.server.lock().unwrap().as_ref().is_some_and(|s| s.last_used.elapsed() > IDLE_STOP);
            if idle {
                state.stop();
                log::info!("engine stopped after idle");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mvp_platform_has_an_engine_build() {
        for (os, arch) in [("linux", "x86_64"), ("linux", "aarch64"), ("macos", "aarch64"), ("windows", "x86_64")] {
            for gpu in [false, true] {
                let s = asset_suffix(os, arch, gpu).unwrap();
                let name = format!("llama-{}-{s}", catalog::catalog().engine.tag);
                assert!(catalog::catalog().engine.assets.contains_key(&name), "нет sha256 для {name}");
            }
        }
        assert_eq!(asset_suffix("macos", "x86_64", false), None, "Mac на Intel не поддерживается");
    }

    #[test]
    fn the_engine_listens_only_locally_and_needs_the_token() {
        let a = launch_args(Path::new("/m/q.gguf"), Some(Path::new("/m/p.gguf")), 4242, "tok", 8192, 4, Device::Gpu);
        let at = |k: &str| a.iter().position(|x| x == k).map(|i| a[i + 1].clone());
        assert_eq!(at("--host").as_deref(), Some("127.0.0.1"));
        assert_eq!(at("--api-key").as_deref(), Some("tok"));
        assert_eq!(at("--port").as_deref(), Some("4242"));
        assert_eq!(at("--mmproj").as_deref(), Some("/m/p.gguf"));
        assert_eq!(at("-ngl").as_deref(), Some("auto"));
        let cpu = launch_args(Path::new("/m/q.gguf"), None, 1, "t", 8192, 4, Device::Cpu);
        assert!(!cpu.contains(&"--mmproj".to_string()));
        assert!(cpu.windows(2).any(|w| w[0] == "--device" && w[1] == "none"));
    }

    #[test]
    fn the_failure_reason_comes_from_the_engine_output() {
        let out = "load_backend: loaded CPU backend\n\nmain: error: failed to load model '/m/q.gguf'\nbye\n";
        assert_eq!(last_lines(out, 2), "main: error: failed to load model '/m/q.gguf'\nbye");
        assert_eq!(reason(&last_lines(out, 12)), ": main: error: failed to load model '/m/q.gguf'");
        assert_eq!(reason("all good"), "");
    }

    #[test]
    fn tokens_are_long_and_different() {
        let (a, b) = (new_token(), new_token());
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
    }

    /// The whole chain on a real engine and a real (small) model: download with the pinned sha256,
    /// unpack, start, a text answer and a picture answer. Needs internet and ~1 GB, so it runs in CI
    /// (`cargo test --lib -- --ignored smoke`), on every OS of the MVP.
    #[test]
    #[ignore]
    fn smoke_real_engine_answers_text_and_pictures() {
        use crate::chat::{request_body, sse_deltas, Msg};
        let cancel = AtomicBool::new(false);
        let log = |stage: &'static str| move |done: u64, total: u64, check: bool| {
            if done == total {
                eprintln!("{stage}: {total} bytes{}", if check { " checked" } else { "" });
            }
        };
        tauri::async_runtime::block_on(async {
            let bin = install(false, &cancel, &log("engine")).await.expect("движок не установился");
            let m = catalog::catalog().models.iter().find(|m| m.id == "qwen3.5-0.8b").unwrap();
            let (file, proj) = model_paths(m).unwrap();
            download::download(&m.file.url, &file, &m.file.sha256, &cancel, &log("model")).await.expect("модель не скачалась");
            let proj = proj.unwrap();
            let p = m.mmproj.as_ref().unwrap();
            download::download(&p.url, &proj, &p.sha256, &cancel, &log("vision")).await.expect("модуль зрения не скачался");

            let (mut child, port, token) = launch(&bin, &file, Some(&proj), 4096, Device::Cpu).await.expect("движок не запустился");
            let client = crate::net::loopback_client().unwrap();
            let ask = |msgs: Vec<Msg>| {
                let client = client.clone();
                let token = token.clone();
                async move {
                    let mut body = request_body(&msgs, 0.2);
                    body["max_tokens"] = serde_json::json!(48);
                    let r = client.post(format!("http://127.0.0.1:{port}/v1/chat/completions")).bearer_auth(&token).json(&body).send().await.unwrap();
                    assert!(r.status().is_success(), "движок ответил {}", r.status());
                    let mut buf = String::new();
                    let (pieces, _) = sse_deltas(&mut buf, &format!("{}\n", r.text().await.unwrap()));
                    pieces.concat()
                }
            };
            // without the token the engine refuses: other programs cannot use the model
            let anon = client.post(format!("http://127.0.0.1:{port}/v1/chat/completions")).json(&request_body(&[], 0.2)).send().await.unwrap();
            assert_eq!(anon.status().as_u16(), 401, "без токена движок должен отказать");

            let text = ask(vec![Msg { role: "user".into(), content: "Сколько будет 2+2? Ответь числом.".into(), images: vec![] }]).await;
            eprintln!("text answer: {text}");
            assert!(!text.trim().is_empty(), "пустой ответ на текст");

            // a 32x32 red PNG, built here so the test needs no files
            let png = tiny_png();
            let url = format!("data:image/png;base64,{}", base64::Engine::encode(&base64::engine::general_purpose::STANDARD, png));
            let seen = ask(vec![Msg { role: "user".into(), content: "Какого цвета эта картинка? Одно слово.".into(), images: vec![url] }]).await;
            eprintln!("vision answer: {seen}");
            assert!(!seen.trim().is_empty(), "пустой ответ на картинку");
            let _ = child.kill();
        });
    }

    /// A 32×32 solid red PNG (zlib stored blocks, no compressor needed).
    #[cfg(test)]
    fn tiny_png() -> Vec<u8> {
        fn crc(data: &[u8]) -> u32 {
            let mut c = 0xffff_ffffu32;
            for &b in data {
                c ^= b as u32;
                for _ in 0..8 {
                    c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
                }
            }
            !c
        }
        fn chunk(out: &mut Vec<u8>, kind: &[u8], data: &[u8]) {
            out.extend((data.len() as u32).to_be_bytes());
            let mut body = kind.to_vec();
            body.extend(data);
            out.extend(&body);
            out.extend(crc(&body).to_be_bytes());
        }
        let (w, h) = (32u32, 32u32);
        // each row: filter byte 0, then RGB pixels
        let row: Vec<u8> = std::iter::once(0u8).chain([220u8, 30, 30].repeat(w as usize)).collect();
        let raw = row.repeat(h as usize);
        let mut z = vec![0x78, 0x01, 1];
        z.extend((raw.len() as u16).to_le_bytes());
        z.extend((!(raw.len() as u16)).to_le_bytes());
        z.extend(&raw);
        let (mut a, mut b) = (1u32, 0u32);
        for &x in &raw {
            a = (a + x as u32) % 65521;
            b = (b + a) % 65521;
        }
        z.extend(((b << 16) | a).to_be_bytes());
        let mut out = vec![0x89, b'P', b'N', b'G', 13, 10, 26, 10];
        let mut ihdr = Vec::new();
        ihdr.extend(w.to_be_bytes());
        ihdr.extend(h.to_be_bytes());
        ihdr.extend([8, 2, 0, 0, 0]);
        chunk(&mut out, b"IHDR", &ihdr);
        chunk(&mut out, b"IDAT", &z);
        chunk(&mut out, b"IEND", &[]);
        out
    }

    #[test]
    fn the_test_picture_is_a_valid_png() {
        let p = tiny_png();
        assert_eq!(&p[..8], &[0x89, b'P', b'N', b'G', 13, 10, 26, 10]);
        assert_eq!(&p[12..16], b"IHDR");
    }

    #[test]
    fn more_memory_gets_a_longer_context() {
        assert_eq!(ctx_size(8 << 30), 8192);
        assert_eq!(ctx_size(16 << 30), 16384);
    }
}
