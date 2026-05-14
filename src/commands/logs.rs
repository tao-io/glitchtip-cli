use anyhow::{Context, Result};

use crate::api::ApiClient;
use crate::api::client::LogFilter;
use crate::config::Config;
use crate::utils::truncate_chars;

pub struct ListOptions<'a> {
    pub level: &'a [String],
    pub service: Option<&'a str>,
    pub environment: Option<&'a str>,
    pub query: Option<&'a str>,
    pub start: Option<&'a str>,
    pub end: Option<&'a str>,
    pub limit: Option<u64>,
}

pub async fn list(config: &Config, opts: &ListOptions<'_>) -> Result<()> {
    let org = config.get_org()?;
    let client = ApiClient::from_config(config)?;

    let filter = LogFilter {
        level: opts.level.to_vec(),
        service: opts.service.map(String::from),
        environment: opts.environment.map(String::from),
        query: opts.query.map(String::from),
        start: opts.start.map(String::from),
        end: opts.end.map(String::from),
        limit: opts.limit,
    };

    let logs = client
        .list_logs(org, &filter)
        .await
        .context("Failed to list logs")?;

    if logs.is_empty() {
        println!("No log entries found.");
        return Ok(());
    }

    println!("{:<28} {:<8} {:<20} Body", "Timestamp", "Level", "Service");
    for entry in logs {
        let body = truncate_chars(&entry.body, 80);
        println!(
            "{:<28} {:<8} {:<20} {}",
            entry.timestamp, entry.level, entry.service, body
        );
    }

    Ok(())
}
