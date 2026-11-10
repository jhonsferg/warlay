//! Signing in with Google from the desktop app (RFC 8252, "native apps").
//!
//! The app listens on a random port of the loopback interface, opens the
//! system browser on the website's sign-in page and waits. When the player is
//! done, the website sends the browser to `http://127.0.0.1:{port}/callback`
//! with a one-time code. The app never sees Google's own tokens: it hands that
//! code and its PKCE verifier to the Warlay server for a session, so the code
//! is no use to anything else that could read the loopback request.

use base64::Engine;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Notify;

/// How long the player has to finish at the browser.
const WAIT: Duration = Duration::from_secs(5 * 60);

/// The flow in progress, so "cancel" (or a second attempt) can end it.
static CURRENT: Mutex<Option<Arc<Notify>>> = Mutex::new(None);

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleGrant {
    pub code: String,
    pub verifier: String,
}

#[derive(Debug, PartialEq, Eq)]
enum Callback {
    Code(String),
    Failure(String),
}

fn url_safe(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// 32 random bytes as URL-safe text.
fn random_text() -> String {
    let mut bytes = [0_u8; 32];
    bytes[..16].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
    bytes[16..].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
    url_safe(&bytes)
}

fn challenge_of(verifier: &str) -> String {
    url_safe(&Sha256::digest(verifier.as_bytes()))
}

/// The answer in a request line like `GET /callback?code=..&state=.. HTTP/1.1`,
/// only if it is for this flow (`state` matches). Anything else (a favicon, a
/// stray request, a forged state) is ignored.
fn parse_callback(request_line: &str, expected_state: &str) -> Option<Callback> {
    let mut parts = request_line.split_whitespace();
    if parts.next()? != "GET" {
        return None;
    }
    let url = reqwest::Url::parse(&format!("http://127.0.0.1{}", parts.next()?)).ok()?;
    if url.path() != "/callback" {
        return None;
    }
    let query = |name: &str| url.query_pairs().find(|(k, _)| k == name).map(|(_, v)| v.into_owned());
    if query("state")? != expected_state {
        return None;
    }
    match (query("code"), query("error")) {
        (Some(code), _) if !code.is_empty() => Some(Callback::Code(code)),
        (_, Some(error)) => Some(Callback::Failure(error)),
        _ => Some(Callback::Failure("provider_invalid".into())),
    }
}

/// The page the browser is left on.
fn done_page(ok: bool, spanish: bool) -> String {
    let (title, text) = match (ok, spanish) {
        (true, true) => ("Listo", "Ya puedes cerrar esta pestaña y volver a Warlay."),
        (true, false) => ("Done", "You can close this tab and go back to Warlay."),
        (false, true) => ("No se pudo iniciar sesión", "Vuelve a Warlay e inténtalo de nuevo."),
        (false, false) => ("Could not sign in", "Go back to Warlay and try again."),
    };
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>Warlay</title>\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"></head>\
         <body style=\"font:16px system-ui;background:#14110b;color:#f1ead8;display:grid;place-items:center;height:100vh;margin:0\">\
         <main style=\"text-align:center\"><h1 style=\"color:#f0cf7e\">{title}</h1><p>{text}</p></main></body></html>"
    )
}

async fn respond(stream: &mut TcpStream, status: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}

/// Reads one request's first line; `None` if it is not a request.
async fn request_line(stream: &mut TcpStream) -> Option<String> {
    let mut buffer = Vec::with_capacity(1024);
    let mut chunk = [0_u8; 1024];
    let read = async {
        while buffer.len() < 8192 && !buffer.windows(2).any(|w| w == b"\r\n") {
            let n = stream.read(&mut chunk).await.ok()?;
            if n == 0 {
                break;
            }
            buffer.extend_from_slice(&chunk[..n]);
        }
        Some(())
    };
    tokio::time::timeout(Duration::from_secs(5), read).await.ok()??;
    let text = String::from_utf8_lossy(&buffer);
    text.lines().next().map(str::to_string)
}

/// Opens the browser. Only the Warlay website (https, or plain http on this
/// machine for development) is ever opened.
fn open_browser(url: &str) -> Result<(), String> {
    let allowed =
        url.starts_with("https://") || url.starts_with("http://127.0.0.1") || url.starts_with("http://localhost");
    if !allowed {
        return Err("invalid_url".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        // `explorer` takes the URL as a single argument: no shell parsing.
        std::process::Command::new("explorer")
            .arg(url)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("open_failed: {e}"))?;
    }
    Ok(())
}

/// Runs the whole browser round trip and returns what the app needs to ask the
/// server for a session. Errors are short codes the interface translates:
/// `cancelled`, `timeout`, and the website's own (`access_denied`,
/// `email_not_verified`, `account_conflict`, ...).
#[tauri::command]
pub async fn google_sign_in(web_url: String, lang: String) -> Result<GoogleGrant, String> {
    let cancel = Arc::new(Notify::new());
    {
        let mut current = CURRENT.lock().expect("sign-in lock");
        if let Some(previous) = current.replace(cancel.clone()) {
            previous.notify_one();
        }
    }
    let result = run(&web_url, &lang, &cancel).await;
    let mut current = CURRENT.lock().expect("sign-in lock");
    if current.as_ref().is_some_and(|c| Arc::ptr_eq(c, &cancel)) {
        *current = None;
    }
    result
}

async fn run(web_url: &str, lang: &str, cancel: &Notify) -> Result<GoogleGrant, String> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .map_err(|e| format!("listen_failed: {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let state = random_text();
    let verifier = random_text();
    let spanish = !lang.starts_with("en");
    let mut url = reqwest::Url::parse(web_url).map_err(|_| "invalid_url".to_string())?;
    url.set_path("/auth/google/start");
    url.query_pairs_mut()
        .clear()
        .append_pair("port", &port.to_string())
        .append_pair("state", &state)
        .append_pair("challenge", &challenge_of(&verifier))
        .append_pair("lang", if spanish { "es" } else { "en" });
    open_browser(url.as_str())?;

    let wait = async {
        loop {
            let Ok((mut stream, peer)) = listener.accept().await else {
                continue;
            };
            // Only this machine can be the browser.
            if !peer.ip().is_loopback() {
                continue;
            }
            let Some(line) = request_line(&mut stream).await else {
                continue;
            };
            match parse_callback(&line, &state) {
                Some(Callback::Code(code)) => {
                    respond(&mut stream, "200 OK", &done_page(true, spanish)).await;
                    return Ok(GoogleGrant { code, verifier });
                }
                Some(Callback::Failure(reason)) => {
                    respond(&mut stream, "200 OK", &done_page(false, spanish)).await;
                    return Err(reason);
                }
                None => respond(&mut stream, "404 Not Found", "").await,
            }
        }
    };
    tokio::select! {
        result = wait => result,
        _ = cancel.notified() => Err("cancelled".into()),
        _ = tokio::time::sleep(WAIT) => Err("timeout".into()),
    }
}

/// The player gave up waiting.
#[tauri::command]
pub fn google_sign_in_cancel() {
    if let Some(current) = CURRENT.lock().expect("sign-in lock").take() {
        current.notify_one();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_this_flows_callback_is_accepted() {
        let ok = parse_callback("GET /callback?state=abc&code=xyz HTTP/1.1", "abc");
        assert_eq!(ok, Some(Callback::Code("xyz".into())));
        let denied = parse_callback("GET /callback?state=abc&error=access_denied HTTP/1.1", "abc");
        assert_eq!(denied, Some(Callback::Failure("access_denied".into())));
        // A forged or missing state, another path or method: ignored.
        assert_eq!(parse_callback("GET /callback?state=zzz&code=xyz HTTP/1.1", "abc"), None);
        assert_eq!(parse_callback("GET /callback?code=xyz HTTP/1.1", "abc"), None);
        assert_eq!(parse_callback("GET /favicon.ico HTTP/1.1", "abc"), None);
        assert_eq!(parse_callback("POST /callback?state=abc&code=x HTTP/1.1", "abc"), None);
        assert_eq!(parse_callback("", "abc"), None);
        // A matching state with nothing useful is a failure, not a hang.
        assert_eq!(
            parse_callback("GET /callback?state=abc HTTP/1.1", "abc"),
            Some(Callback::Failure("provider_invalid".into()))
        );
    }

    #[test]
    fn the_challenge_matches_the_servers_rule() {
        // RFC 7636 appendix B.
        assert_eq!(
            challenge_of("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        assert_ne!(random_text(), random_text());
        assert_eq!(random_text().len(), 43);
    }

    #[test]
    fn only_websites_open_in_the_browser() {
        assert!(open_browser("file:///C:/Windows/System32/calc.exe").is_err());
        assert!(open_browser("javascript:alert(1)").is_err());
        assert!(open_browser("http://evil.example/").is_err());
    }

    #[tokio::test]
    async fn the_loopback_round_trip_returns_the_code() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let line = request_line(&mut stream).await.unwrap();
            let outcome = parse_callback(&line, "st");
            respond(&mut stream, "200 OK", &done_page(true, false)).await;
            outcome
        });
        let body = reqwest::get(format!("http://127.0.0.1:{port}/callback?state=st&code=c1"))
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(body.contains("close this tab"));
        assert_eq!(server.await.unwrap(), Some(Callback::Code("c1".into())));
    }
}
