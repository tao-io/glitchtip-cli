use anyhow::Result;
use serde::Serialize;

use crate::api::ApiClient;
use crate::config::Config;

#[derive(Serialize)]
struct ConfigStatus {
    config: ConfigInfo,
    auth: AuthStatus,
}

#[derive(Serialize)]
struct ConfigInfo {
    org: Option<String>,
    project: Option<String>,
    url: String,
}

#[derive(Serialize)]
struct AuthStatus {
    #[serde(rename = "type")]
    auth_type: String,
    successful: bool,
}

pub async fn execute(config: &Config, config_status_json: bool, no_defaults: bool) -> Result<()> {
    if config_status_json {
        return print_json(config).await;
    }

    println!("Sentry Server: {}", config.url);

    if !no_defaults {
        println!(
            "Default Organization: {}",
            config.org.as_deref().unwrap_or("(not set)")
        );
        println!(
            "Default Project: {}",
            config.project.as_deref().unwrap_or("(not set)")
        );
    }

    println!();

    if config.auth_token.is_none() {
        println!("Authentication Info:");
        println!("  Method: Unauthorized");
        println!("  No auth token configured. Run `glitchtip-cli login` to set one.");
        anyhow::bail!("Not authenticated");
    }

    let client = ApiClient::from_config(config)?;
    match client.get_auth_info().await {
        Ok(auth_info) => {
            println!("Authentication Info:");
            if config.oauth.is_some() {
                println!("  Method: OAuth");
            } else {
                println!("  Method: Auth Token");
            }
            if let Some(user) = &auth_info.user
                && let Some(email) = &user.email
            {
                println!("  User: {email}");
            }
            if let Some(auth) = &auth_info.auth
                && !auth.scopes.is_empty()
            {
                println!("  Scopes:");
                for scope in &auth.scopes {
                    println!("    - {scope}");
                }
            }
        }
        Err(e) => {
            println!("Authentication Info:");
            println!("  Method: Auth Token (INVALID)");
            println!("  Error: {e}");
            anyhow::bail!("Authentication failed");
        }
    }

    Ok(())
}

async fn print_json(config: &Config) -> Result<()> {
    let has_token = config.auth_token.is_some();
    let mut auth_successful = false;
    let mut auth_type = "none".to_string();

    if has_token {
        auth_type = if config.oauth.is_some() {
            "oauth".to_string()
        } else {
            "token".to_string()
        };
        let client = ApiClient::from_config(config)?;
        auth_successful = client.get_auth_info().await.is_ok();
    }

    let status = ConfigStatus {
        config: ConfigInfo {
            org: config.org.clone(),
            project: config.project.clone(),
            url: config.url.clone(),
        },
        auth: AuthStatus {
            auth_type,
            successful: auth_successful,
        },
    };

    println!(
        "{}",
        serde_json::to_string_pretty(&status).expect("Failed to serialize config status")
    );

    if !auth_successful && has_token {
        anyhow::bail!("Authentication failed");
    }

    Ok(())
}
