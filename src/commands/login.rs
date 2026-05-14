use std::io::{self, Write};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};

use crate::api::ApiClient;
use crate::config::Config;
use crate::oauth::{self, OAuthTokens};

pub async fn execute(config: &Config, global: bool, method: &str) -> Result<()> {
    match method {
        "token" => execute_token_login(config, global).await,
        "oauth" => execute_oauth_login(config, global).await,
        _ => anyhow::bail!("Unknown login method: {method}. Use 'token' or 'oauth'."),
    }
}

async fn execute_token_login(config: &Config, global: bool) -> Result<()> {
    println!("This helps you sign in your CLI with an authentication token.");
    println!(
        "If you do not yet have a token ready we can bring up a browser for you\nto create a token now."
    );
    println!();
    println!("GlitchTip server: {}", config.url.trim_end_matches('/'));
    println!();

    if prompt_yes_no("Open browser now?")? {
        let token_url = format!("{}profile/auth-tokens/", config.url);
        println!("Opening: {token_url}");
        if let Err(e) = open::that(&token_url) {
            eprintln!("Could not open browser: {e}");
            eprintln!("Please open this URL manually: {token_url}");
        }
        println!();
    }

    let token = prompt_for_token()?;
    println!();

    // Validate the token
    let mut test_config = config.clone();
    test_config.auth_token = Some(token.clone());
    let client = ApiClient::from_config(&test_config)?;

    print!("Verifying auth token... ");
    io::stdout().flush()?;

    match client.get_auth_info().await {
        Ok(auth_info) => {
            if let Some(user) = &auth_info.user {
                let id = user
                    .email
                    .as_deref()
                    .or(user.name.as_deref())
                    .unwrap_or("unknown");
                println!("valid (user: {id})");
            } else {
                println!("valid (org token)");
            }
        }
        Err(e) => {
            println!("FAILED");
            anyhow::bail!("Invalid auth token: {e}");
        }
    }

    // Save the token
    let path = Config::config_path(global)?;
    Config::save_token(&path, &token)
        .with_context(|| format!("Failed to save token to {}", path.display()))?;

    println!();
    println!("Token saved to {}", path.display());

    Ok(())
}

async fn execute_oauth_login(config: &Config, _global: bool) -> Result<()> {
    // OAuth tokens are always saved to the global config path so the daemon-free
    // refresh logic in main.rs can locate them on startup. --global is accepted
    // for symmetry with the token login flow but has no effect here.
    let base_url = config.url.trim_end_matches('/');
    println!("Logging in via OAuth to {base_url}");
    println!();

    let http_client = reqwest::Client::new();

    // 1. Discover server metadata
    print!("Discovering OAuth server... ");
    io::stdout().flush()?;
    let metadata = oauth::discover_server(base_url, &http_client).await?;
    println!("ok");

    // 2. Start local callback server
    let (listener, _port, redirect_uri) = oauth::start_callback_server()?;

    // 3. Register client dynamically
    let registration_endpoint = metadata
        .registration_endpoint
        .context("Server does not support dynamic client registration")?;
    print!("Registering client... ");
    io::stdout().flush()?;
    let client_id =
        oauth::register_client(&registration_endpoint, &redirect_uri, &http_client).await?;
    println!("ok");

    // 4. Generate PKCE verifier + challenge
    let verifier = oauth::generate_code_verifier();
    let challenge = oauth::compute_code_challenge(&verifier);
    let state = oauth::generate_state();

    // 5. Build auth URL and open browser
    let auth_url = oauth::build_auth_url(
        &metadata.authorization_endpoint,
        &client_id,
        &redirect_uri,
        &state,
        &challenge,
    );
    println!("Opening browser for authorization...");
    if let Err(e) = open::that(&auth_url) {
        eprintln!("Could not open browser: {e}");
        println!("Please open this URL manually:");
        println!("  {auth_url}");
    }

    // 6. Wait for callback
    println!("Waiting for authorization (timeout: 120s)...");
    let code = oauth::wait_for_callback(listener, &state, 120).await?;

    // 7. Exchange code for tokens
    print!("Exchanging authorization code... ");
    io::stdout().flush()?;
    let token_response = oauth::exchange_code(
        &metadata.token_endpoint,
        &code,
        &client_id,
        &verifier,
        &redirect_uri,
        &http_client,
    )
    .await?;
    println!("ok");

    // 8. Validate by calling get_auth_info
    let mut test_config = config.clone();
    test_config.auth_token = Some(token_response.access_token.clone());
    let api_client = ApiClient::from_config(&test_config)?;

    print!("Verifying token... ");
    io::stdout().flush()?;
    match api_client.get_auth_info().await {
        Ok(auth_info) => {
            if let Some(user) = &auth_info.user {
                let id = user
                    .email
                    .as_deref()
                    .or(user.name.as_deref())
                    .unwrap_or("unknown");
                println!("valid (user: {id})");
            } else {
                println!("valid");
            }
        }
        Err(e) => {
            println!("FAILED");
            anyhow::bail!("OAuth token validation failed: {e}");
        }
    }

    // 9. Save tokens to config
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let expires_at = now + token_response.expires_in.unwrap_or(3600);

    let tokens = OAuthTokens {
        access_token: token_response.access_token,
        refresh_token: token_response
            .refresh_token
            .context("Server did not return a refresh token")?,
        client_id,
        expires_at,
        scopes: token_response.scope.unwrap_or_default(),
        token_endpoint: metadata.token_endpoint,
    };

    // OAuth always saves globally — .sentryclirc is read-only compat
    let path = Config::config_path(true)?;
    Config::save_oauth_tokens(&path, &tokens)
        .with_context(|| format!("Failed to save OAuth tokens to {}", path.display()))?;

    println!();
    println!("OAuth tokens saved to {}", path.display());

    Ok(())
}

fn prompt_yes_no(message: &str) -> Result<bool> {
    loop {
        print!("{message} [y/n] ");
        io::stdout().flush()?;

        let mut buf = String::new();
        io::stdin().read_line(&mut buf)?;
        match buf.trim().to_lowercase().as_str() {
            "y" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            _ => println!("Please enter y or n."),
        }
    }
}

fn prompt_for_token() -> Result<String> {
    print!("Enter your token: ");
    io::stdout().flush()?;

    let token = rpassword::read_password().context("Failed to read token")?;
    let token = token.trim().to_string();

    if token.is_empty() {
        anyhow::bail!("No token provided");
    }

    Ok(token)
}
