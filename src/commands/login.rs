use std::io::{self, Write};

use anyhow::{Context, Result};

use crate::api::ApiClient;
use crate::config::Config;

pub async fn execute(config: &Config, global: bool) -> Result<()> {
    println!("This helps you sign in your CLI with an authentication token.");
    println!(
        "If you do not yet have a token ready we can bring up a browser for you\nto create a token now."
    );
    println!();
    println!("Sentry server: {}", config.url.trim_end_matches('/'));
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
