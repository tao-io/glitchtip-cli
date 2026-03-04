use anyhow::{Context, Result};

use crate::api::ApiClient;
use crate::config::Config;

pub async fn list(config: &Config) -> Result<()> {
    let org = config.get_org()?;

    let client = ApiClient::from_config(config)?;
    let mut repos = client
        .list_repositories(org)
        .await
        .context("Failed to list repositories")?;

    if repos.is_empty() {
        println!("No repositories found.");
        return Ok(());
    }

    // Sort by name
    repos.sort_by(|a, b| a.name.cmp(&b.name));

    println!("{:<30} {:<20} {:<15}", "Name", "Provider", "Status");
    for repo in repos {
        let provider = if let Some(p) = &repo.provider {
            p.get("name")
                .and_then(|n| n.as_str())
                .unwrap_or(p.get("id").and_then(|i| i.as_str()).unwrap_or("-"))
        } else {
            "-"
        };
        let status = repo.status.as_deref().unwrap_or("-");
        println!("{:<30} {:<20} {:<15}", repo.name, provider, status);
    }

    Ok(())
}
