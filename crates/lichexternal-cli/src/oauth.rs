//! Lichess OAuth 2.0 with PKCE (Proof Key for Code Exchange) implementation.
//!
//! Handles ephemeral loopback HTTP server, PKCE verifier/challenge generation,
//! browser launch, authorization code capture, and token exchange.

use anyhow::{bail, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use ring::digest::{digest, SHA256};
use ring::rand::{SecureRandom, SystemRandom};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

pub const DEFAULT_CLIENT_ID: &str = "chexperts-lichexternal";
pub const DEFAULT_SCOPES: &str = "engine:read engine:write";

/// Ephemeral PKCE challenge and verifier pair.
#[derive(Debug, Clone)]
pub struct PkcePair {
    pub verifier: String,
    pub challenge: String,
}

impl PkcePair {
    /// Generates a cryptographically secure random PKCE code verifier and S256 challenge.
    pub fn generate() -> Result<Self> {
        let rng = SystemRandom::new();
        let mut random_bytes = [0u8; 32];
        rng.fill(&mut random_bytes)
            .map_err(|_| anyhow::anyhow!("Failed to acquire secure random bytes for PKCE"))?;

        let verifier = URL_SAFE_NO_PAD.encode(random_bytes);
        let challenge_hash = digest(&SHA256, verifier.as_bytes());
        let challenge = URL_SAFE_NO_PAD.encode(challenge_hash.as_ref());

        Ok(Self {
            verifier,
            challenge,
        })
    }
}

/// Token response returned by `POST https://lichess.org/api/token`.
#[derive(Debug, Clone, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    #[allow(dead_code)]
    pub token_type: Option<String>,
    #[allow(dead_code)]
    pub expires_in: Option<u64>,
    pub scope: Option<String>,
}

/// Account profile response from `GET /api/account`.
#[derive(Debug, Clone, Deserialize)]
pub struct AccountResponse {
    #[allow(dead_code)]
    pub id: String,
    pub username: String,
}

/// Executes interactive OAuth 2.0 PKCE login with Lichess.
pub async fn login_pkce(
    base_url: &str,
    client_id: &str,
) -> Result<(TokenResponse, Option<String>)> {
    let pkce = PkcePair::generate().context("Failed to generate PKCE challenge")?;

    // Bind local ephemeral TCP listener on loopback
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .context("Failed to bind ephemeral loopback TCP listener for OAuth redirect")?;
    let local_addr = listener.local_addr()?;
    let redirect_uri = format!("http://127.0.0.1:{}/callback", local_addr.port());

    // Authorization URL
    let auth_url = format!(
        "{}/oauth?response_type=code&client_id={}&redirect_uri={}&code_challenge={}&code_challenge_method=S256&scope={}",
        base_url.trim_end_matches('/'),
        url_encode(client_id),
        url_encode(&redirect_uri),
        url_encode(&pkce.challenge),
        url_encode(DEFAULT_SCOPES)
    );

    println!("\nInitiating Lichess OAuth 2.0 Authorization...\n");
    println!("Opening your default browser to authorize Lichexternal:");
    println!("  {auth_url}\n");

    // Attempt to open browser automatically
    if let Err(e) = open_browser(&auth_url) {
        println!("Note: Could not open browser automatically: {e}");
        println!("Please manually copy and paste the URL above into your browser.");
    }

    println!(
        "Waiting for Lichess authorization callback on {}...",
        redirect_uri
    );

    // Wait for the authorization code from the loopback server
    let (code_tx, code_rx) = oneshot::channel::<Result<String>>();
    let code_tx = Arc::new(tokio::sync::Mutex::new(Some(code_tx)));

    tokio::select! {
        res = listen_for_callback(listener, code_tx) => {
            if let Err(e) = res {
                bail!("Callback listener failed: {e}");
            }
        }
        code_result = code_rx => {
            let code = code_result.context("Failed to receive authorization code from callback")??;
            println!("✓ Authorization code received! Exchanging for access token...");

            let token_resp = exchange_code_for_token(base_url, client_id, &code, &redirect_uri, &pkce.verifier).await?;
            let username = fetch_account_username(base_url, &token_resp.access_token).await.ok();

            return Ok((token_resp, username));
        }
    }

    bail!("OAuth flow terminated prematurely")
}

async fn listen_for_callback(
    listener: TcpListener,
    code_tx: Arc<tokio::sync::Mutex<Option<oneshot::Sender<Result<String>>>>>,
) -> Result<()> {
    while let Ok((mut socket, _)) = listener.accept().await {
        let mut buf = [0u8; 2048];
        let n = match socket.read(&mut buf).await {
            Ok(n) if n > 0 => n,
            _ => continue,
        };

        let request = String::from_utf8_lossy(&buf[..n]);
        let first_line = request.lines().next().unwrap_or("");
        let mut parts = first_line.split_whitespace();
        let _method = parts.next();
        let path = parts.next().unwrap_or("/");

        let (code, error) = parse_query_params(path);

        if let Some(err_msg) = error {
            let html = format!(
                "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html; charset=utf-8\r\n\r\n\
                <html><body style='font-family:sans-serif;text-align:center;padding:50px;'>\
                <h2 style='color:#e53e3e;'>Authentication Failed</h2>\
                <p>{}</p>\
                <p>You can return to your terminal.</p>\
                </body></html>",
                err_msg
            );
            let _ = socket.write_all(html.as_bytes()).await;
            if let Some(tx) = code_tx.lock().await.take() {
                let _ = tx.send(Err(anyhow::anyhow!("OAuth error from Lichess: {err_msg}")));
            }
            break;
        } else if let Some(auth_code) = code {
            let html = "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\r\n\
                <html><body style='font-family:sans-serif;text-align:center;padding:50px;'>\
                <h2 style='color:#38a169;'>Authentication Successful!</h2>\
                <p>Lichexternal has been authorized. You can close this browser tab.</p>\
                </body></html>";
            let _ = socket.write_all(html.as_bytes()).await;
            if let Some(tx) = code_tx.lock().await.take() {
                let _ = tx.send(Ok(auth_code));
            }
            break;
        } else {
            let html = "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\n\r\nNot Found";
            let _ = socket.write_all(html.as_bytes()).await;
        }
    }
    Ok(())
}

fn parse_query_params(uri: &str) -> (Option<String>, Option<String>) {
    let Some(query_idx) = uri.find('?') else {
        return (None, None);
    };
    let query_str = &uri[query_idx + 1..];
    let mut params = HashMap::new();

    for pair in query_str.split('&') {
        let mut split = pair.splitn(2, '=');
        if let (Some(k), Some(v)) = (split.next(), split.next()) {
            params.insert(k, v);
        }
    }

    let code = params.get("code").map(|s| s.to_string());
    let error = params.get("error").map(|s| s.to_string());
    (code, error)
}

async fn exchange_code_for_token(
    base_url: &str,
    client_id: &str,
    code: &str,
    redirect_uri: &str,
    code_verifier: &str,
) -> Result<TokenResponse> {
    let token_url = format!("{}/api/token", base_url.trim_end_matches('/'));
    let client = reqwest::Client::new();

    let params = [
        ("grant_type", "authorization_code"),
        ("client_id", client_id),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("code_verifier", code_verifier),
    ];

    let resp = client
        .post(&token_url)
        .form(&params)
        .send()
        .await
        .with_context(|| format!("Failed to reach Lichess token endpoint '{token_url}'"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!("Lichess token exchange failed ({status}): {body}");
    }

    let token_resp: TokenResponse = resp
        .json()
        .await
        .context("Failed to deserialize Lichess token response")?;

    Ok(token_resp)
}

pub async fn fetch_account_username(base_url: &str, access_token: &str) -> Result<String> {
    let account_url = format!("{}/api/account", base_url.trim_end_matches('/'));
    let client = reqwest::Client::new();

    let resp = client
        .get(&account_url)
        .bearer_auth(access_token)
        .send()
        .await
        .with_context(|| format!("Failed to reach Lichess account endpoint '{account_url}'"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        bail!("Failed to fetch account info ({status})");
    }

    let acc: AccountResponse = resp
        .json()
        .await
        .context("Failed to parse account response")?;
    Ok(acc.username)
}

fn url_encode(input: &str) -> String {
    let mut encoded = String::with_capacity(input.len());
    for b in input.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(b as char);
            }
            _ => {
                encoded.push_str(&format!("%{:02X}", b));
            }
        }
    }
    encoded
}

fn open_browser(url: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(url).spawn()?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/c", "start", "", url])
            .spawn()?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open").arg(url).spawn()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pkce_generation() {
        let pair = PkcePair::generate().expect("Should generate PKCE pair");
        assert!(!pair.verifier.is_empty());
        assert!(!pair.challenge.is_empty());
        assert_ne!(pair.verifier, pair.challenge);
    }

    #[test]
    fn test_url_encode() {
        assert_eq!(url_encode("hello world"), "hello%20world");
        assert_eq!(
            url_encode("engine:read engine:write"),
            "engine%3Aread%20engine%3Awrite"
        );
    }

    #[test]
    fn test_parse_query_params() {
        let (code, err) = parse_query_params("/callback?code=abc123xyz&state=something");
        assert_eq!(code, Some("abc123xyz".to_string()));
        assert_eq!(err, None);

        let (code, err) = parse_query_params("/callback?error=access_denied");
        assert_eq!(code, None);
        assert_eq!(err, Some("access_denied".to_string()));
    }
}
