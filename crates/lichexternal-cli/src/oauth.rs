//! Lichess OAuth 2.0 with PKCE (Proof Key for Code Exchange) implementation.
//!
//! Handles ephemeral loopback HTTP server, PKCE verifier/challenge generation,
//! CSRF state protection, browser launch, authorization code capture, and token exchange.

use anyhow::{bail, Context, Result};
use oauth2::basic::BasicClient;
use oauth2::reqwest::async_http_client;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, CsrfToken, PkceCodeChallenge, RedirectUrl, Scope,
    TokenResponse as _, TokenUrl,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use url::Url;

pub const DEFAULT_CLIENT_ID: &str = "chexperts-lichexternal";
pub const DEFAULT_SCOPES: &[&str] = &["engine:read", "engine:write"];

/// Token response details needed by Lichexternal.
#[derive(Debug, Clone)]
pub struct TokenDetails {
    pub access_token: String,
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
pub async fn login_pkce(base_url: &str, client_id: &str) -> Result<(TokenDetails, Option<String>)> {
    let clean_base = base_url.trim_end_matches('/');
    let auth_url = AuthUrl::new(format!("{clean_base}/oauth"))
        .context("Invalid authorization endpoint URL")?;
    let token_url =
        TokenUrl::new(format!("{clean_base}/api/token")).context("Invalid token endpoint URL")?;

    // Bind local ephemeral TCP listener on loopback
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .context("Failed to bind ephemeral loopback TCP listener for OAuth redirect")?;
    let local_addr = listener.local_addr()?;
    let redirect_url_str = format!("http://127.0.0.1:{}/callback", local_addr.port());
    let redirect_url =
        RedirectUrl::new(redirect_url_str.clone()).context("Invalid redirect URI")?;

    let client = BasicClient::new(
        ClientId::new(client_id.to_string()),
        None,
        auth_url,
        Some(token_url),
    )
    .set_redirect_uri(redirect_url);

    // Generate PKCE challenge and CSRF token
    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    let mut auth_req = client.authorize_url(CsrfToken::new_random);
    for scope in DEFAULT_SCOPES {
        auth_req = auth_req.add_scope(Scope::new(scope.to_string()));
    }
    let (authorize_url, csrf_state) = auth_req.set_pkce_challenge(pkce_challenge).url();

    println!("\nInitiating Lichess OAuth 2.0 Authorization...\n");
    println!("Opening your default browser to authorize Lichexternal:");
    println!("  {authorize_url}\n");

    if let Err(e) = open::that(authorize_url.as_str()) {
        println!("Note: Could not open browser automatically: {e}");
        println!("Please manually copy and paste the URL above into your browser.");
    }

    println!(
        "Waiting for Lichess authorization callback on {}...",
        redirect_url_str
    );

    let (code_tx, code_rx) = oneshot::channel::<Result<AuthorizationCode>>();
    let code_tx = Arc::new(tokio::sync::Mutex::new(Some(code_tx)));
    let expected_state = csrf_state.secret().clone();

    tokio::select! {
        res = listen_for_callback(listener, code_tx, expected_state) => {
            if let Err(e) = res {
                bail!("Callback listener failed: {e}");
            }
        }
        code_result = code_rx => {
            let auth_code = code_result.context("Failed to receive authorization code from callback")??;
            println!("✓ Authorization code received! Exchanging for access token...");

            let token_res = client
                .exchange_code(auth_code)
                .set_pkce_verifier(pkce_verifier)
                .request_async(async_http_client)
                .await
                .context("Lichess OAuth token exchange request failed")?;

            let access_token = token_res.access_token().secret().to_string();
            let scope = token_res.scopes().map(|s| {
                s.iter()
                    .map(|scope| scope.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            });

            let token_details = TokenDetails {
                access_token: access_token.clone(),
                scope,
            };

            let username = fetch_account_username(base_url, &access_token).await.ok();

            return Ok((token_details, username));
        }
    }

    bail!("OAuth flow terminated prematurely")
}

async fn listen_for_callback(
    listener: TcpListener,
    code_tx: Arc<tokio::sync::Mutex<Option<oneshot::Sender<Result<AuthorizationCode>>>>>,
    expected_state: String,
) -> Result<()> {
    while let Ok((mut socket, _)) = listener.accept().await {
        let mut buf = [0u8; 4096];
        let n = match socket.read(&mut buf).await {
            Ok(n) if n > 0 => n,
            _ => continue,
        };

        let request = String::from_utf8_lossy(&buf[..n]);
        let first_line = request.lines().next().unwrap_or("");
        let mut parts = first_line.split_whitespace();
        let _method = parts.next();
        let path = parts.next().unwrap_or("/");

        let (code, state, error) = parse_query_params(path);

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
        }

        if let (Some(auth_code), Some(incoming_state)) = (code, state) {
            if incoming_state != expected_state {
                let html =
                    "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html; charset=utf-8\r\n\r\n\
                    <html><body style='font-family:sans-serif;text-align:center;padding:50px;'>\
                    <h2 style='color:#e53e3e;'>CSRF Validation Failed</h2>\
                    <p>OAuth state mismatch. Request rejected for security.</p>\
                    </body></html>";
                let _ = socket.write_all(html.as_bytes()).await;
                if let Some(tx) = code_tx.lock().await.take() {
                    let _ = tx.send(Err(anyhow::anyhow!(
                        "CSRF state mismatch during OAuth callback"
                    )));
                }
                break;
            }

            let html = "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\r\n\
                <html><body style='font-family:sans-serif;text-align:center;padding:50px;'>\
                <h2 style='color:#38a169;'>Authentication Successful!</h2>\
                <p>Lichexternal has been authorized. You can close this browser tab.</p>\
                </body></html>";
            let _ = socket.write_all(html.as_bytes()).await;
            if let Some(tx) = code_tx.lock().await.take() {
                let _ = tx.send(Ok(AuthorizationCode::new(auth_code)));
            }
            break;
        }

        let html = "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\n\r\nNot Found";
        let _ = socket.write_all(html.as_bytes()).await;
    }
    Ok(())
}

fn parse_query_params(uri: &str) -> (Option<String>, Option<String>, Option<String>) {
    let dummy_base = "http://localhost";
    let full_url = format!("{dummy_base}{uri}");
    let Ok(parsed) = Url::parse(&full_url) else {
        return (None, None, None);
    };

    let params: HashMap<String, String> = parsed.query_pairs().into_owned().collect();
    let code = params.get("code").cloned();
    let state = params.get("state").cloned();
    let error = params.get("error").cloned();

    (code, state, error)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_query_params() {
        let (code, state, err) =
            parse_query_params("/callback?code=abc123xyz&state=my_csrf_secret");
        assert_eq!(code, Some("abc123xyz".to_string()));
        assert_eq!(state, Some("my_csrf_secret".to_string()));
        assert_eq!(err, None);

        let (code, state, err) = parse_query_params("/callback?error=access_denied");
        assert_eq!(code, None);
        assert_eq!(state, None);
        assert_eq!(err, Some("access_denied".to_string()));
    }
}
