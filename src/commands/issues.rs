use anyhow::{Context, Result};

use crate::api::ApiClient;
use crate::config::Config;

pub async fn list(config: &Config) -> Result<()> {
    let org = config
        .org
        .as_deref()
        .context("Organization not set. Use --org, SENTRY_ORG, or set it in .sentryclirc")?;

    let project = config
        .project
        .as_deref()
        .context("Project not set. Use --project, SENTRY_PROJECT, or set it in .sentryclirc")?;

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
        println!("{:<20} {:<50} {:<15} {:<10}", short_id, title, status, level);
    }

    Ok(())
}
