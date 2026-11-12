//! Downloads of big files (the engine, models): several connections when the server supports
//! Range, resume after a break, sha256 check before the file is put in place, cancellation.
//!
//! Origin: adapted from OpsDeck `src-tauri/src/ai.rs` (same author).

use crate::store::err;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

/// How many connections to open per file (like aria2c -x8) and from which size it makes sense at all.
/// Hugging Face's CDN caps a single connection well below a typical home line.
const DL_PARTS: u64 = 8;
const DL_MIN_SPLIT: u64 = 16 * 1024 * 1024;

/// Progress of one file: bytes done of total; `check` — the sha256 pass after the download.
pub trait Progress: Fn(u64, u64, bool) + Send + Sync {}
impl<T: Fn(u64, u64, bool) + Send + Sync> Progress for T {}

/// What is already in the .part: per part, how many bytes reached the disk. A break (network,
/// quitting the app) no longer costs the whole model: next time the parts continue where they stopped.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
struct DlState {
    url: String,
    total: u64,
    /// per part: start, end (inclusive) and how much from the start is written
    parts: Vec<(u64, u64, u64)>,
}

fn state_path(part: &Path) -> PathBuf {
    part.with_extension("part.state")
}

/// The saved state, if it is about the same file and matches its size.
fn load_state(part: &Path, url: &str, total: u64) -> Option<DlState> {
    let st: DlState = serde_json::from_str(&fs::read_to_string(state_path(part)).ok()?).ok()?;
    let same_file = st.url == url && st.total == total && fs::metadata(part).ok()?.len() == total;
    let sane = st.parts.iter().all(|(a, b, d)| b >= a && *d <= b - a + 1);
    (same_file && sane && !st.parts.is_empty()).then_some(st)
}

fn save_state(part: &Path, st: &DlState) {
    if let Ok(text) = serde_json::to_string(st) {
        let _ = fs::write(state_path(part), text);
    }
}

fn drop_state(part: &Path) {
    let _ = fs::remove_file(state_path(part));
}

/// Range bounds (inclusive) for `Range: bytes=a-b`. The last part takes the rest.
fn ranges(total: u64, parts: u64) -> Vec<(u64, u64)> {
    if total == 0 || parts <= 1 {
        return vec![(0, total.saturating_sub(1))];
    }
    // no more parts than bytes: otherwise the step is zero and the ranges turn inside out
    let parts = parts.min(total);
    let step = total / parts;
    (0..parts).map(|i| (i * step, if i + 1 == parts { total - 1 } else { (i + 1) * step - 1 })).collect()
}

/// Does the server give pieces by Range? The answer to a probe of the first byte is 206 with the size.
fn range_total(status: u16, content_range: Option<&str>) -> Option<u64> {
    if status != 206 {
        return None;
    }
    // "bytes 0-0/5680522464"
    content_range?.rsplit('/').next()?.trim().parse().ok().filter(|n| *n > 0)
}

/// Bounds and full size from a server's `Content-Range` response header.
fn parse_content_range(value: &str) -> Option<(u64, u64, u64)> {
    let value = value.strip_prefix("bytes ")?;
    let (bounds, total) = value.split_once('/')?;
    let (from, to) = bounds.split_once('-')?;
    Some((from.parse().ok()?, to.parse().ok()?, total.parse().ok()?))
}

/// sha256 of a written file: it cannot be computed on the fly, the parts arrive out of order.
pub fn sha_of(path: &Path, cancel: &AtomicBool, on_progress: &dyn Fn(u64)) -> Result<String, String> {
    use std::io::Read;
    let mut f = fs::File::open(path).map_err(err)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 4 * 1024 * 1024];
    let mut done = 0u64;
    let mut last = Instant::now();
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("загрузка отменена".into());
        }
        let n = f.read(&mut buf).map_err(err)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        done += n as u64;
        if last.elapsed() > Duration::from_millis(250) {
            on_progress(done);
            last = Instant::now();
        }
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// A file in several connections: each part is written to its place in one file (positional
/// writes), progress is the sum over the parts. Needs a server that supports Range.
async fn download_parts(
    client: &reqwest::Client,
    url: &str,
    part: &Path,
    total: u64,
    parts: u64,
    cancel: &AtomicBool,
    on_progress: &(dyn Fn(u64) + Send + Sync),
) -> Result<(), String> {
    // continue the previous .part if it is about this file; otherwise start over
    let resumed = load_state(part, url, total).filter(|st| st.parts.len() as u64 == parts.min(total.max(1)));
    if resumed.is_none() {
        let f = fs::File::create(part).map_err(err)?;
        f.set_len(total).map_err(err)?;
        drop(f);
        drop_state(part);
    }
    let plan: Vec<(u64, u64, u64)> = resumed
        .map(|st| st.parts)
        .unwrap_or_else(|| ranges(total, parts).into_iter().map(|(a, b)| (a, b, 0)).collect());
    let already: u64 = plan.iter().map(|(.., d)| *d).sum();

    // per part: how much is written (for the UI) and how much of it is on disk (for resuming)
    let live: Vec<_> = plan.iter().map(|(.., d)| Arc::new(AtomicU64::new(*d))).collect();
    let safe: Vec<_> = live.iter().map(|d| Arc::new(AtomicU64::new(d.load(Ordering::Relaxed)))).collect();
    let sum = |xs: &[Arc<AtomicU64>]| xs.iter().map(|x| x.load(Ordering::Relaxed)).sum::<u64>();
    let snapshot = |xs: &[Arc<AtomicU64>]| DlState {
        url: url.to_string(),
        total,
        parts: plan.iter().zip(xs).map(|((a, b, _), d)| (*a, *b, d.load(Ordering::Relaxed))).collect(),
    };

    let mut set = tokio::task::JoinSet::new();
    for (i, &(from, to, done_before)) in plan.iter().enumerate() {
        if from + done_before > to {
            continue; // this part was fully downloaded last time
        }
        let (client, url, path) = (client.clone(), url.to_string(), part.to_path_buf());
        let (live, safe) = (live[i].clone(), safe[i].clone());
        set.spawn(async move {
            const MAX_RETRIES: u8 = 4;
            let mut at = from + done_before;
            let mut failures = 0u8;
            let mut f = fs::OpenOptions::new().write(true).open(&path).map_err(err)?;
            // only what is on disk goes to the resume state: after a hard break we would otherwise
            // count a tail that was still in the cache
            let publish = |f: &mut fs::File, at: u64| {
                let _ = f.sync_data();
                safe.store(at - from, Ordering::Relaxed);
            };
            loop {
                if cancel.load(Ordering::Relaxed) {
                    publish(&mut f, at);
                    return Err("загрузка отменена".into());
                }
                let requested_from = at;
                let response = client
                    .get(&url)
                    .header(reqwest::header::ACCEPT_ENCODING, "identity")
                    .header(reqwest::header::RANGE, format!("bytes={requested_from}-{to}"))
                    .send()
                    .await;
                let mut resp = match response {
                    Ok(resp) if resp.status() == reqwest::StatusCode::PARTIAL_CONTENT => resp,
                    Ok(resp) => return Err(format!("сервер не поддержал диапазон загрузки: {}", resp.status())),
                    Err(e) => {
                        failures += 1;
                        if failures > MAX_RETRIES {
                            publish(&mut f, at);
                            return Err(format!("загрузка не удалась после повторов: {e}"));
                        }
                        tokio::time::sleep(Duration::from_millis(250 * (1u64 << (failures - 1)))).await;
                        continue;
                    }
                };
                let bounds = resp
                    .headers()
                    .get(reqwest::header::CONTENT_RANGE)
                    .and_then(|v| v.to_str().ok())
                    .and_then(parse_content_range);
                let Some((response_from, response_to, response_total)) = bounds else {
                    return Err("сервер прислал неверный Content-Range при загрузке".into());
                };
                if response_from != requested_from || response_to > to || response_total != total || response_to < response_from {
                    return Err("сервер прислал диапазон, отличный от запрошенного".into());
                }

                let mut unsynced = 0u64;
                let failure = loop {
                    if cancel.load(Ordering::Relaxed) {
                        publish(&mut f, at);
                        return Err("загрузка отменена".into());
                    }
                    match resp.chunk().await {
                        Ok(Some(chunk)) => {
                            if at + chunk.len() as u64 > response_to + 1 {
                                return Err("сервер прислал больше байт, чем указано в Content-Range".into());
                            }
                            write_at(&mut f, at, &chunk)?;
                            at += chunk.len() as u64;
                            unsynced += chunk.len() as u64;
                            live.store(at - from, Ordering::Relaxed);
                            if unsynced >= 2 * 1024 * 1024 {
                                publish(&mut f, at);
                                unsynced = 0;
                            }
                        }
                        Ok(None) if at == response_to + 1 => break None,
                        Ok(None) => break Some("соединение закрылось раньше времени".to_string()),
                        Err(e) => break Some(format!("загрузка прервалась: {e}")),
                    }
                };
                publish(&mut f, at);
                if at > to {
                    return Ok(());
                }
                if at > requested_from {
                    failures = 0;
                }
                if let Some(e) = failure {
                    failures += 1;
                    if failures > MAX_RETRIES {
                        return Err(format!("{e}; повторите установку модели"));
                    }
                    tokio::time::sleep(Duration::from_millis(250 * (1u64 << (failures - 1)))).await;
                }
            }
        });
    }
    let mut saved = Instant::now();
    loop {
        if cancel.load(Ordering::Relaxed) {
            set.shutdown().await;
            save_state(part, &snapshot(&safe));
            return Err("загрузка отменена".into());
        }
        let step = tokio::time::timeout(Duration::from_millis(250), set.join_next()).await;
        let all_done = matches!(step, Ok(None));
        if let Ok(Some(res)) = step {
            let failed = match res {
                Err(e) => Some(format!("загрузка прервалась: {e}")),
                Ok(Err(e)) => Some(e),
                Ok(Ok(())) => None,
            };
            if let Some(e) = failed {
                set.shutdown().await;
                // what is downloaded stays: the next attempt continues from there
                save_state(part, &snapshot(&safe));
                return Err(e);
            }
        }
        on_progress(already.min(total).max(sum(&live)));
        if saved.elapsed() > Duration::from_secs(2) {
            save_state(part, &snapshot(&safe));
            saved = Instant::now();
        }
        if all_done {
            break;
        }
    }
    let got = sum(&live);
    if got != total {
        save_state(part, &snapshot(&safe));
        return Err(format!("скачано {got} из {total} байт — соединение оборвалось"));
    }
    drop_state(part);
    on_progress(total);
    Ok(())
}

/// Write at an offset: write_at on Unix, seek_write on Windows (both from std).
fn write_at(f: &mut fs::File, at: u64, buf: &[u8]) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileExt;
        f.write_all_at(buf, at).map_err(err)
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileExt;
        let mut off = at;
        let mut rest = buf;
        while !rest.is_empty() {
            let n = f.seek_write(rest, off).map_err(err)?;
            if n == 0 {
                return Err("запись в файл не удалась".into());
            }
            off += n as u64;
            rest = &rest[n..];
        }
        Ok(())
    }
}

/// A client for downloads from the internet (system proxy settings apply).
pub fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(concat!("OpsOpsAi/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(20))
        .build()
        .map_err(err)
}

/// Downloads `url` to `dest` (via a .part file next to it) and checks the sha256 before putting
/// the file in place. A server that supports Range is used in several connections and can resume.
pub async fn download(url: &str, dest: &Path, sha256: &str, cancel: &AtomicBool, on_progress: &dyn Progress) -> Result<(), String> {
    let client = client()?;
    if let Some(dir) = dest.parent() {
        fs::create_dir_all(dir).map_err(err)?;
    }
    let part = dest.with_extension("part");
    // a probe of one byte tells both the size and whether the server gives pieces
    let probe = client
        .get(url)
        .header(reqwest::header::RANGE, "bytes=0-0")
        .send()
        .await
        .map_err(|e| format!("нет связи с сервером загрузки: {e}"))?;
    let whole = range_total(probe.status().as_u16(), probe.headers().get(reqwest::header::CONTENT_RANGE).and_then(|v| v.to_str().ok()));
    drop(probe);
    let total = if let Some(total) = whole {
        let parts = if total >= DL_MIN_SPLIT { DL_PARTS } else { 1 };
        download_parts(&client, url, &part, total, parts, cancel, &|done| on_progress(done, total, false)).await?;
        total
    } else {
        let mut resp = client.get(url).send().await.map_err(|e| format!("нет связи с сервером загрузки: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("сервер загрузки ответил {}", resp.status()));
        }
        let total = resp.content_length().unwrap_or(0);
        let mut f = fs::File::create(&part).map_err(err)?;
        let mut done = 0u64;
        while let Some(chunk) = resp.chunk().await.map_err(|e| format!("загрузка прервалась: {e}"))? {
            if cancel.load(Ordering::Relaxed) {
                drop(f);
                let _ = fs::remove_file(&part);
                return Err("загрузка отменена".into());
            }
            f.write_all(&chunk).map_err(err)?;
            done += chunk.len() as u64;
            on_progress(done, total, false);
        }
        f.flush().map_err(err)?;
        done
    };
    let got = sha_of(&part, cancel, &|done| on_progress(done, total, true))?;
    if !got.eq_ignore_ascii_case(sha256) {
        let _ = fs::remove_file(&part);
        drop_state(&part);
        return Err("контрольная сумма не совпала — файл повреждён или подменён, он удалён".into());
    }
    fs::rename(&part, dest).map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    fn sha(body: &[u8]) -> String {
        let mut h = Sha256::new();
        h.update(body);
        format!("{:x}", h.finalize())
    }

    /// A whole request: one read may bring half the headers, and an empty connection is not a request.
    fn read_request(s: &mut std::net::TcpStream) -> Option<String> {
        let mut req = String::new();
        let mut buf = [0u8; 1024];
        loop {
            match s.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => req.push_str(&String::from_utf8_lossy(&buf[..n])),
            }
            if req.contains("\r\n\r\n") {
                break;
            }
        }
        (!req.trim().is_empty()).then_some(req)
    }

    fn range_of(req: &str) -> Option<(u64, u64)> {
        let r = req.lines().find(|l| l.to_ascii_lowercase().starts_with("range:"))?.split('=').nth(1)?.trim().to_string();
        let (a, b) = r.split_once('-')?;
        Some((a.parse().ok()?, b.parse().ok()?))
    }

    fn test_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("opsopsai-dl-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    /// Four connections assemble exactly the file the server gave: if ranges or offsets drifted,
    /// the sha256 would show it.
    #[test]
    fn four_connections_assemble_the_same_file() {
        let body: Vec<u8> = (0..300_000u32).flat_map(|i| i.to_le_bytes()).collect();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let srv = {
            let body = body.clone();
            std::thread::spawn(move || {
                let mut served = 0;
                while served < 4 {
                    let Ok((mut s, _)) = listener.accept() else { return };
                    let Some(req) = read_request(&mut s) else { continue };
                    served += 1;
                    let (from, to) = range_of(&req).unwrap();
                    let slice = &body[from as usize..=to as usize];
                    let head = format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {from}-{to}/{}\r\nConnection: close\r\n\r\n",
                        slice.len(),
                        body.len()
                    );
                    let _ = s.write_all(head.as_bytes()).and_then(|_| s.write_all(slice));
                }
            })
        };
        let dir = test_dir("parts");
        let part = dir.join("model.part");
        let cancel = AtomicBool::new(false);
        let seen = std::sync::Mutex::new(Vec::<u64>::new());
        let url = format!("http://127.0.0.1:{port}/model.gguf");
        tauri::async_runtime::block_on(download_parts(&reqwest::Client::new(), &url, &part, body.len() as u64, 4, &cancel, &|d| {
            seen.lock().unwrap().push(d)
        }))
        .unwrap();
        srv.join().unwrap();
        assert_eq!(sha_of(&part, &cancel, &|_| {}).unwrap(), sha(&body), "файл собран не так, как отдавал сервер");
        assert_eq!(*seen.lock().unwrap().last().unwrap(), body.len() as u64);
        fs::remove_dir_all(&dir).unwrap();
    }

    /// A break does not cost the whole model: the next attempt asks only for the rest.
    #[test]
    fn an_interrupted_download_resumes_where_it_stopped() {
        let body: Vec<u8> = (0..200_000u32).flat_map(|i| i.to_le_bytes()).collect();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let asked: Arc<std::sync::Mutex<Vec<(u64, u64)>>> = Default::default();
        // the test says which attempt is running: under load a connection of the first attempt can
        // reach the server late, and counting by order would mistake it for the second attempt
        let second = Arc::new(AtomicBool::new(false));
        let srv = {
            let (body, asked, second) = (body.clone(), asked.clone(), second.clone());
            std::thread::spawn(move || {
                let mut served_second = 0;
                while served_second < 4 {
                    let Ok((mut s, _)) = listener.accept() else { return };
                    let Some(req) = read_request(&mut s) else { continue };
                    let (from, to) = range_of(&req).unwrap();
                    let phase2 = second.load(Ordering::SeqCst);
                    // first attempt: each answer gives a quarter of its range and breaks off
                    let cut = if phase2 { to - from + 1 } else { (to - from + 1) / 4 };
                    if phase2 {
                        asked.lock().unwrap().push((from, to));
                        served_second += 1;
                    }
                    let slice = &body[from as usize..(from + cut) as usize];
                    let head = format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {from}-{to}/{}\r\nConnection: close\r\n\r\n",
                        to - from + 1,
                        body.len()
                    );
                    let _ = s.write_all(head.as_bytes()).and_then(|_| s.write_all(slice));
                }
            })
        };
        let dir = test_dir("resume");
        let part = dir.join("model.part");
        let cancel = AtomicBool::new(false);
        let client = reqwest::Client::builder().pool_max_idle_per_host(0).build().unwrap();
        let url = format!("http://127.0.0.1:{port}/model.gguf");
        let total = body.len() as u64;

        let first = tauri::async_runtime::block_on(download_parts(&client, &url, &part, total, 4, &cancel, &|_| {}));
        assert!(first.is_err(), "сервер оборвал отдачу — загрузка должна была не дойти");
        let st = load_state(&part, &url, total).expect("состояние докачки не сохранилось");
        let got_before: u64 = st.parts.iter().map(|(.., d)| *d).sum();
        assert!(got_before > 0 && got_before < total);

        second.store(true, Ordering::SeqCst);
        tauri::async_runtime::block_on(download_parts(&client, &url, &part, total, 4, &cancel, &|_| {})).unwrap();
        srv.join().unwrap();
        let again: u64 = asked.lock().unwrap().iter().map(|(a, b)| b - a + 1).sum();
        assert_eq!(again, total - got_before, "докачивается ровно остаток, а не файл заново");
        assert_eq!(sha_of(&part, &cancel, &|_| {}).unwrap(), sha(&body));
        assert!(load_state(&part, &url, total).is_none(), "состояние убирается, когда файл дошёл целиком");
        fs::remove_dir_all(&dir).unwrap();
    }

    /// The whole public path: probe, parts, sha256 check, file put in place; a wrong sha256 leaves nothing.
    #[test]
    fn download_checks_the_sha256_before_putting_the_file_in_place() {
        let body: Vec<u8> = (0..50_000u32).flat_map(|i| i.to_le_bytes()).collect();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let srv = {
            let body = body.clone();
            std::thread::spawn(move || {
                // two downloads, each: probe + one part
                for _ in 0..4 {
                    let Ok((mut s, _)) = listener.accept() else { return };
                    let Some(req) = read_request(&mut s) else { continue };
                    let (from, to) = range_of(&req).unwrap();
                    let slice = &body[from as usize..=to as usize];
                    let head = format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {from}-{to}/{}\r\nConnection: close\r\n\r\n",
                        slice.len(),
                        body.len()
                    );
                    let _ = s.write_all(head.as_bytes()).and_then(|_| s.write_all(slice));
                }
            })
        };
        let dir = test_dir("whole");
        let cancel = AtomicBool::new(false);
        let url = format!("http://127.0.0.1:{port}/m.gguf");
        let good = dir.join("good.gguf");
        tauri::async_runtime::block_on(download(&url, &good, &sha(&body), &cancel, &|_, _, _| {})).unwrap();
        assert_eq!(fs::read(&good).unwrap(), body);
        let bad = dir.join("bad.gguf");
        let res = tauri::async_runtime::block_on(download(&url, &bad, &"0".repeat(64), &cancel, &|_, _, _| {}));
        assert!(res.unwrap_err().contains("контрольная сумма"));
        assert!(!bad.exists() && !bad.with_extension("part").exists(), "битый файл не остаётся на диске");
        srv.join().unwrap();
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn ranges_cover_the_file_exactly() {
        assert_eq!(ranges(1000, 4), [(0, 249), (250, 499), (500, 749), (750, 999)]);
        let r = ranges(1003, 4);
        assert_eq!(r.last().unwrap(), &(750, 1002));
        for w in r.windows(2) {
            assert_eq!(w[1].0, w[0].1 + 1, "дыра или наложение: {w:?}");
        }
        assert_eq!(ranges(10, 1), [(0, 9)]);
        assert_eq!(ranges(0, 4), [(0, 0)]);
        assert_eq!(ranges(3, 4).iter().map(|(a, b)| b - a + 1).sum::<u64>(), 3);
    }

    #[test]
    fn the_probe_tells_the_size_only_for_a_real_range_answer() {
        assert_eq!(range_total(206, Some("bytes 0-0/5680522464")), Some(5680522464));
        assert_eq!(range_total(206, Some("bytes 0-0/*")), None);
        assert_eq!(range_total(200, Some("bytes 0-0/100")), None);
        assert_eq!(range_total(206, None), None);
    }
}
