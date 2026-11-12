//! HTTP to the local engine. Corporate networks set `http_proxy`/`HTTPS_PROXY`, and reqwest honours
//! them for any address, 127.0.0.1 included — a proxy cannot reach our loopback, so requests to the
//! engine go around it.
//!
//! Origin: adapted from OpsDeck `src-tauri/src/net.rs` (same author).

use crate::store::err;
use std::time::Duration;

/// The URL points at this same machine (loopback), even if written as a name.
pub fn loopback(url: &str) -> bool {
    let rest = url.split("://").nth(1).unwrap_or(url);
    let host = rest.split('/').next().unwrap_or("");
    let host = host.rsplit('@').next().unwrap_or(host);
    // [::1]:8080 → ::1, 127.0.0.1:8080 → 127.0.0.1
    let host = if let Some(end) = host.strip_prefix('[').and_then(|h| h.split(']').next()) { end } else { host.split(':').next().unwrap_or(host) };
    let host = host.to_ascii_lowercase();
    // "127.example.com" is an ordinary domain: compare as an address, not as a string
    host == "localhost" || host.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

/// A client for the local engine: no proxy, generous timeouts for long answers.
pub fn loopback_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder().no_proxy().connect_timeout(Duration::from_secs(5)).build().map_err(err)
}

#[cfg(test)]
mod tests {
    use super::loopback;

    #[test]
    fn loopback_is_recognised_with_port_user_and_brackets() {
        for url in ["http://127.0.0.1:8080/health", "http://localhost/v1/models", "http://[::1]:11434/", "https://127.0.0.5:9000"] {
            assert!(loopback(url), "{url}");
        }
        for url in ["http://203.0.113.30:8611/", "https://llm.example.com/v1", "http://127.example.com/", "http://10.0.0.1/"] {
            assert!(!loopback(url), "{url}");
        }
    }
}
