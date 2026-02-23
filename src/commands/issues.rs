use anyhow::{Context, Result, bail};

use crate::api::ApiClient;
use crate::api::client::IssueChanges;
use crate::config::Config;

pub async fn list(config: &Config) -> Result<()> {
    let org = config.get_org()?;
    let project = config.get_project()?;

    let client = ApiClient::from_config(config)?;
    let mut issues = client
        .list_issues(org, project)
        .await
        .context("Failed to list issues")?;

    if issues.is_empty() {
        println!("No issues found.");
        return Ok(());
    }

    // Sort by last_seen descending (most recent first)
    issues.sort_by(|a, b| b.last_seen.cmp(&a.last_seen));

    println!(
        "{:<20} {:<50} {:<15} {:<10}",
        "Short ID", "Title", "Status", "Level"
    );
    for issue in issues {
        let short_id = issue.short_id.as_deref().unwrap_or(&issue.id);
        let title = if issue.title.len() > 48 {
            format!("{}...", &issue.title[..45])
        } else {
            issue.title.clone()
        };
        let status = issue.status.as_deref().unwrap_or("-");
        let level = issue.level.as_deref().unwrap_or("-");
        println!(
            "{:<20} {:<50} {:<15} {:<10}",
            short_id, title, status, level
        );
    }

    Ok(())
}

fn build_filter_query(ids: &[u64], status: Option<&str>, all: bool) -> Result<String> {
    if all {
        return Ok(String::new());
    }

    let mut parts = Vec::new();

    for id in ids {
        parts.push(format!("id={id}"));
    }

    if let Some(s) = status {
        parts.push(format!("status={s}"));
    }

    if parts.is_empty() {
        bail!("No filter specified. Use --id, --status, or --all to select issues.");
    }

    Ok(parts.join("&"))
}

async fn update_status(
    config: &Config,
    ids: &[u64],
    status: Option<&str>,
    all: bool,
    new_status: &str,
    action_label: &str,
) -> Result<()> {
    let org = config.get_org()?;
    let project = config.get_project()?;

    let filter_query = build_filter_query(ids, status, all)?;

    let client = ApiClient::from_config(config)?;
    let changes = IssueChanges {
        new_status: new_status.to_string(),
    };

    client
        .update_issues(org, project, &filter_query, &changes)
        .await
        .with_context(|| format!("Failed to {action_label} issues"))?;

    println!("Issues {action_label}d successfully.");
    Ok(())
}

pub async fn resolve(config: &Config, ids: &[u64], status: Option<&str>, all: bool) -> Result<()> {
    update_status(config, ids, status, all, "resolved", "resolve").await
}

pub async fn mute(config: &Config, ids: &[u64], status: Option<&str>, all: bool) -> Result<()> {
    update_status(config, ids, status, all, "ignored", "mute").await
}

pub async fn unresolve(
    config: &Config,
    ids: &[u64],
    status: Option<&str>,
    all: bool,
) -> Result<()> {
    update_status(config, ids, status, all, "unresolved", "unresolve").await
}
