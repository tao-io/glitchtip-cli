use std::process::Command;

use anyhow::{Context, Result};

use crate::api::ApiClient;
use crate::config::Config;

pub async fn list(config: &Config) -> Result<()> {
    let org = config.get_org()?;
    let client = ApiClient::from_config(config)?;
    let mut monitors = client
        .list_monitors(org)
        .await
        .context("Failed to list monitors")?;

    if monitors.is_empty() {
        println!("No monitors found.");
        return Ok(());
    }

    monitors.sort_by(|a, b| a.name.cmp(&b.name));

    println!(
        "{:<30} {:<12} {:<40} {:<8} {:<10}",
        "Name", "Type", "URL", "Status", "Interval"
    );
    for m in monitors {
        let status = match m.is_up {
            Some(true) => "Up",
            Some(false) => "Down",
            None => "?",
        };
        let url = m.url.as_deref().unwrap_or("-");
        let interval = m
            .interval
            .map(|i| format!("{i}s"))
            .unwrap_or_else(|| "-".to_string());
        println!(
            "{:<30} {:<12} {:<40} {:<8} {:<10}",
            m.name, m.monitor_type, url, status, interval
        );
    }

    Ok(())
}

/// Run a command and send a heartbeat on success. Returns the child's exit code.
pub async fn run(config: &Config, endpoint_id: &str, args: &[String]) -> Result<i32> {
    let org = config.get_org()?;
    let client = ApiClient::from_config(config)?;

    let status = Command::new(&args[0])
        .args(&args[1..])
        .status()
        .with_context(|| format!("Failed to execute command: {}", args[0]))?;

    let exit_code = status.code().unwrap_or(1);

    if exit_code == 0 {
        match client.heartbeat_check(org, endpoint_id).await {
            Ok(_) => eprintln!("Heartbeat sent successfully."),
            Err(e) => eprintln!("Warning: failed to send heartbeat: {e}"),
        }
    } else {
        eprintln!("Command exited with status {exit_code}; heartbeat not sent.");
    }

    Ok(exit_code)
}
