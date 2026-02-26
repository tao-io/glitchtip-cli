use std::process::Command;

use anyhow::{Context, Result};

use crate::api::ApiClient;
use crate::api::client::NewMonitor;
use crate::config::Config;

pub struct CreateOptions<'a> {
    pub name: &'a str,
    pub monitor_type: &'a str,
    pub url: Option<&'a str>,
    pub interval: Option<u64>,
    pub project: Option<&'a str>,
}

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

pub async fn create(config: &Config, opts: &CreateOptions<'_>) -> Result<()> {
    let org = config.get_org()?;
    let client = ApiClient::from_config(config)?;

    let new_monitor = NewMonitor {
        name: opts.name.to_string(),
        monitor_type: opts.monitor_type.to_string(),
        url: opts.url.map(|s| s.to_string()),
        interval: opts.interval,
        project: opts.project.map(|s| s.to_string()),
        expected_body: Some("".to_string()),
        expected_status: Some(200),
        timeout: Some(20),
    };

    let monitor = client
        .create_monitor(org, &new_monitor)
        .await
        .context("Failed to create monitor")?;

    println!("Created monitor: {}", monitor.name);
    if let Some(id) = monitor.id.as_str() {
        println!("ID: {}", id);
    } else if let Some(id_num) = monitor.id.as_u64() {
        println!("ID: {}", id_num);
    } else if let Some(id_val) = monitor.id.as_i64() {
        println!("ID: {}", id_val);
    } else {
        println!("ID: {}", monitor.id);
    }

    if monitor.monitor_type == "Heartbeat" {
        if let Some(endpoint) = monitor.endpoint_id {
            println!("Endpoint ID: {}", endpoint);
        }
    }

    Ok(())
}

pub async fn delete(config: &Config, id: &str) -> Result<()> {
    let org = config.get_org()?;
    let client = ApiClient::from_config(config)?;

    client
        .delete_monitor(org, id)
        .await
        .context("Failed to delete monitor")?;

    println!("Deleted monitor: {}", id);
    Ok(())
}
