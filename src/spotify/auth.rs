//! OAuth 2.0 Authorization Code flow with PKCE, plus token persistence.
//!
//! The redirect is caught by a tiny one-shot HTTP server on 127.0.0.1, so no
//! client secret is ever needed.

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

pub const AUTH_URL: &str = "https://accounts.spotify.com/authorize";
pub const TOKEN_URL: &str = "https://accounts.spotify.com/api/token";

pub const SCOPES: &[&str] = &[
    "user-read-playback-state",
    "user-modify-playback-state",
    "user-read-currently-playing",
    "user-read-private",
    "user-library-read",
    "user-library-modify",
    "user-read-recently-played",
    "user-top-read",
    "user-follow-read",
    "user-follow-modify",
    "playlist-read-private",
    "playlist-read-collaborative",
    "playlist-modify-private",
    "playlist-modify-public",
    "streaming",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenSet {
    pub access_token: String,
    pub refresh_token: String,
    /// Unix seconds.
    pub expires_at: u64,
    #[serde(default)]
    pub scope: String,
    #[serde(default)]
    pub client_id: String,
}

impl TokenSet {
    pub fn expires_soon(&self) -> bool {
        now() + 60 >= self.expires_at
    }
}

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

pub fn load_token(path: &Path) -> Option<TokenSet> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn save_token(path: &Path, token: &TokenSet) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(token)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

pub fn delete_token(path: &Path) {
    let _ = std::fs::remove_file(path);
}

/// A started PKCE authorization.
#[derive(Clone)]
pub struct Pkce {
    pub verifier: String,
    pub state: String,
    pub url: String,
    pub redirect_uri: String,
    pub client_id: String,
}

fn random_string(len: usize) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
    (0..len)
        .map(|_| CHARS[rand::random_range(0..CHARS.len())] as char)
        .collect()
}

pub fn begin(client_id: &str, redirect_uri: &str, scopes: &[&str]) -> Pkce {
    let verifier = random_string(96);
    let digest = sha2::Sha256::digest(verifier.as_bytes());
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
    let state = random_string(24);
    let mut url = url::Url::parse(AUTH_URL).expect("static url");
    url.query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("code_challenge_method", "S256")
        .append_pair("code_challenge", &challenge)
        .append_pair("state", &state)
        .append_pair("scope", &scopes.join(" "));
    Pkce {
        verifier,
        state,
        url: url.to_string(),
        redirect_uri: redirect_uri.to_string(),
        client_id: client_id.to_string(),
    }
}

const PAGE_OK: &str = r#"<!doctype html><html><head><meta charset="utf-8"><title>Spotr</title>
<style>body{font-family:ui-monospace,monospace;background:#111;color:#eee;display:flex;align-items:center;justify-content:center;height:100vh;margin:0}
div{text-align:center}h1{color:#1db954;letter-spacing:.3em}</style></head>
<body><div><h1>SPOTR</h1><p>You're connected. You can close this tab and return to your terminal.</p></div></body></html>"#;

const PAGE_ERR: &str = r#"<!doctype html><html><head><meta charset="utf-8"><title>Spotr</title></head>
<body style="font-family:monospace;background:#111;color:#eee;text-align:center;padding-top:30vh">
<h1 style="color:#e55">Authorization failed</h1><p>Return to Spotr for details.</p></body></html>"#;

/// Listen on the redirect URI's port and wait for Spotify to send the code.
pub async fn wait_for_code(pkce: &Pkce, timeout: Duration) -> Result<String> {
    let url = url::Url::parse(&pkce.redirect_uri).context("invalid redirect URI")?;
    let port = url.port().unwrap_or(80);
    let host = url.host_str().unwrap_or("127.0.0.1").to_string();
    let path = url.path().to_string();
    let listener = TcpListener::bind((host.as_str(), port))
        .await
        .with_context(|| format!("could not listen on {host}:{port} (is another Spotr running?)"))?;

    let fut = async {
        loop {
            let (mut sock, _) = listener.accept().await?;
            let mut buf = vec![0u8; 8192];
            let mut len = 0;
            loop {
                let n = sock.read(&mut buf[len..]).await?;
                if n == 0 {
                    break;
                }
                len += n;
                if buf[..len].windows(4).any(|w| w == b"\r\n\r\n") || len == buf.len() {
                    break;
                }
            }
            let req = String::from_utf8_lossy(&buf[..len]).to_string();
            let target = req.split_whitespace().nth(1).unwrap_or("/").to_string();
            let full = url::Url::parse(&format!("http://localhost{target}"))?;
            if full.path() != path {
                let _ = sock
                    .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                    .await;
                continue;
            }
            let params: std::collections::HashMap<String, String> =
                full.query_pairs().into_owned().collect();
            let respond = |ok: bool| {
                let body = if ok { PAGE_OK } else { PAGE_ERR };
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                )
            };
            if let Some(err) = params.get("error") {
                let _ = sock.write_all(respond(false).as_bytes()).await;
                bail!("Spotify denied access: {err}");
            }
            if params.get("state") != Some(&pkce.state) {
                let _ = sock.write_all(respond(false).as_bytes()).await;
                bail!("state mismatch in OAuth callback");
            }
            let code = params.get("code").cloned().ok_or_else(|| anyhow!("missing code"))?;
            let _ = sock.write_all(respond(true).as_bytes()).await;
            let _ = sock.flush().await;
            return Ok::<String, anyhow::Error>(code);
        }
    };
    tokio::time::timeout(timeout, fut)
        .await
        .map_err(|_| anyhow!("timed out waiting for the browser"))?
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    expires_in: u64,
    #[serde(default)]
    scope: String,
}

#[derive(Deserialize)]
struct TokenError {
    error: String,
    #[serde(default)]
    error_description: Option<String>,
}

async fn token_request(http: &reqwest::Client, form: &[(&str, &str)]) -> Result<TokenResponse> {
    let resp = http.post(TOKEN_URL).form(form).send().await.context("contacting Spotify")?;
    let status = resp.status();
    let text = resp.text().await?;
    if !status.is_success() {
        if let Ok(e) = serde_json::from_str::<TokenError>(&text) {
            bail!("{}{}", e.error, e.error_description.map(|d| format!(": {d}")).unwrap_or_default());
        }
        bail!("token request failed ({status})");
    }
    Ok(serde_json::from_str(&text)?)
}

pub async fn exchange(http: &reqwest::Client, pkce: &Pkce, code: &str) -> Result<TokenSet> {
    let r = token_request(
        http,
        &[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", &pkce.redirect_uri),
            ("client_id", &pkce.client_id),
            ("code_verifier", &pkce.verifier),
        ],
    )
    .await?;
    Ok(TokenSet {
        access_token: r.access_token,
        refresh_token: r.refresh_token.unwrap_or_default(),
        expires_at: now() + r.expires_in,
        scope: r.scope,
        client_id: pkce.client_id.clone(),
    })
}

pub async fn refresh(http: &reqwest::Client, client_id: &str, refresh_token: &str) -> Result<TokenSet> {
    let r = token_request(
        http,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", client_id),
        ],
    )
    .await?;
    Ok(TokenSet {
        access_token: r.access_token,
        // Spotify may rotate the refresh token.
        refresh_token: r.refresh_token.unwrap_or_else(|| refresh_token.to_string()),
        expires_at: now() + r.expires_in,
        scope: r.scope,
        client_id: client_id.to_string(),
    })
}

/// Full interactive login: open the browser, wait for the redirect, exchange the code.
pub async fn login(http: &reqwest::Client, pkce: &Pkce, open_browser: bool) -> Result<TokenSet> {
    if open_browser {
        let _ = open::that_detached(&pkce.url);
    }
    let code = wait_for_code(pkce, Duration::from_secs(300)).await?;
    exchange(http, pkce, &code).await
}

pub fn valid_client_id(id: &str) -> bool {
    id.len() == 32 && id.chars().all(|c| c.is_ascii_hexdigit())
}
