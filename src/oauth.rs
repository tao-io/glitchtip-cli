use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;

use anyhow::{Context, Result};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::Rng;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// OAuth scopes requested by the CLI.
const REQUESTED_SCOPES: &str =
    "org:read org:write project:read project:write project:releases event:read event:write";

// --- Structs ---

#[derive(Debug, Deserialize)]
pub struct ServerMetadata {
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub registration_endpoint: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ClientRegistration {
    pub client_id: String,
}

#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: Option<u64>,
    pub scope: Option<String>,
}

/// Persisted OAuth token state for the [oauth] INI section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub client_id: String,
    pub expires_at: u64,
    pub scopes: String,
    pub token_endpoint: String,
}

// --- Discovery ---

/// Fetch the OAuth Authorization Server Metadata (RFC 8414).
pub async fn discover_server(base_url: &str, client: &Client) -> Result<ServerMetadata> {
    let url = format!(
        "{}/.well-known/oauth-authorization-server",
        base_url.trim_end_matches('/')
    );
    let resp = client
        .get(&url)
        .send()
        .await
        .context("Failed to fetch OAuth server metadata")?;
    if !resp.status().is_success() {
        anyhow::bail!("OAuth discovery failed: HTTP {} from {url}", resp.status());
    }
    resp.json()
        .await
        .context("Failed to parse OAuth server metadata")
}

// --- Dynamic Client Registration (RFC 7591) ---

/// Register a new OAuth client dynamically.
pub async fn register_client(
    endpoint: &str,
    redirect_uri: &str,
    client: &Client,
) -> Result<String> {
    let body = serde_json::json!({
        "redirect_uris": [redirect_uri],
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
        "client_name": "glitchtip-cli",
        "scope": REQUESTED_SCOPES,
        "token_endpoint_auth_method": "none",
    });
    let resp = client
        .post(endpoint)
        .json(&body)
        .send()
        .await
        .context("Failed to register OAuth client")?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Client registration failed: HTTP {status}: {text}");
    }
    let reg: ClientRegistration = resp
        .json()
        .await
        .context("Failed to parse client registration response")?;
    Ok(reg.client_id)
}

// --- PKCE (RFC 7636) ---

/// Generate a random code_verifier (43-128 chars, base64url).
pub fn generate_code_verifier() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Compute S256 code_challenge from a verifier.
pub fn compute_code_challenge(verifier: &str) -> String {
    let hash = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(hash)
}

// --- Local Callback Server ---

/// Bind a TCP listener on a random port and return (listener, port, redirect_uri).
pub fn start_callback_server() -> Result<(TcpListener, u16, String)> {
    let listener =
        TcpListener::bind("127.0.0.1:0").context("Failed to bind local callback server")?;
    let port = listener.local_addr()?.port();
    let redirect_uri = format!("http://127.0.0.1:{port}/callback");
    Ok((listener, port, redirect_uri))
}

/// Wait for the OAuth callback, extract the authorization code.
/// Sends a simple HTML response telling the user to close the tab.
pub async fn wait_for_callback(
    listener: TcpListener,
    expected_state: &str,
    timeout_secs: u64,
) -> Result<String> {
    let expected = expected_state.to_string();

    // Move blocking accept into a spawn_blocking task with timeout
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs),
        tokio::task::spawn_blocking(move || accept_callback(listener, &expected)),
    )
    .await;

    match result {
        Ok(Ok(code)) => code,
        Ok(Err(e)) => Err(anyhow::anyhow!("Callback server task failed: {e}")),
        Err(_) => anyhow::bail!("Timed out waiting for OAuth callback ({}s)", timeout_secs),
    }
}

fn accept_callback(listener: TcpListener, expected_state: &str) -> Result<String> {
    let (mut stream, _) = listener.accept().context("Failed to accept connection")?;

    let mut reader = BufReader::new(&stream);
    let mut request_line = String::new();
    reader
        .read_line(&mut request_line)
        .context("Failed to read HTTP request")?;

    // Parse "GET /callback?code=...&state=... HTTP/1.1"
    let path = request_line
        .split_whitespace()
        .nth(1)
        .context("Invalid HTTP request")?;

    let query = path.split_once('?').map(|(_, q)| q).unwrap_or_default();

    let mut code = None;
    let mut state = None;
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            match k {
                "code" => code = Some(urlencoding::decode(v)?.into_owned()),
                "state" => state = Some(urlencoding::decode(v)?.into_owned()),
                _ => {}
            }
        }
    }

    // Verify state
    let received_state = state.context("No state parameter in callback")?;
    if received_state != expected_state {
        anyhow::bail!("State mismatch: expected {expected_state}, got {received_state}");
    }

    let auth_code = code.context("No code parameter in callback")?;

    // Send response
    let html = "<html><body><h1>Authentication successful!</h1><p>You can close this tab and return to the terminal.</p></body></html>";
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        html.len(),
        html,
    );
    stream.write_all(response.as_bytes()).ok();
    stream.flush().ok();

    Ok(auth_code)
}

// --- Token Exchange ---

/// Exchange an authorization code for tokens.
pub async fn exchange_code(
    endpoint: &str,
    code: &str,
    client_id: &str,
    verifier: &str,
    redirect_uri: &str,
    client: &Client,
) -> Result<TokenResponse> {
    let resp = client
        .post(endpoint)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("client_id", client_id),
            ("code_verifier", verifier),
            ("redirect_uri", redirect_uri),
        ])
        .send()
        .await
        .context("Failed to exchange authorization code")?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Token exchange failed: HTTP {status}: {text}");
    }
    resp.json().await.context("Failed to parse token response")
}

/// Refresh an access token using a refresh token.
pub async fn refresh_tokens(
    endpoint: &str,
    refresh_token: &str,
    client_id: &str,
    client: &Client,
) -> Result<TokenResponse> {
    let resp = client
        .post(endpoint)
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", client_id),
        ])
        .send()
        .await
        .context("Failed to refresh tokens")?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Token refresh failed: HTTP {status}: {text}");
    }
    resp.json()
        .await
        .context("Failed to parse refresh token response")
}

/// Build the authorization URL with PKCE parameters.
pub fn build_auth_url(
    authorization_endpoint: &str,
    client_id: &str,
    redirect_uri: &str,
    state: &str,
    code_challenge: &str,
) -> String {
    format!(
        "{authorization_endpoint}?response_type=code&client_id={}&redirect_uri={}&state={}&code_challenge={}&code_challenge_method=S256&scope={}",
        urlencoding::encode(client_id),
        urlencoding::encode(redirect_uri),
        urlencoding::encode(state),
        urlencoding::encode(code_challenge),
        urlencoding::encode(REQUESTED_SCOPES),
    )
}

/// Generate a random state parameter for CSRF protection.
pub fn generate_state() -> String {
    let mut bytes = [0u8; 16];
    rand::rng().fill(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_verifier_length() {
        let verifier = generate_code_verifier();
        assert_eq!(verifier.len(), 43); // 32 bytes -> 43 base64url chars
    }

    #[test]
    fn test_code_challenge_s256() {
        // RFC 7636 Appendix B test vector
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = compute_code_challenge(verifier);
        assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn test_state_is_unique() {
        let s1 = generate_state();
        let s2 = generate_state();
        assert_ne!(s1, s2);
    }
}
