use anyhow::{Context, Result};

use crate::api::{ApiClient, CreateRepositoryRequest};
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

pub async fn create(
    config: &Config,
    name: &str,
    url: Option<&str>,
    provider: Option<&str>,
) -> Result<()> {
    let org = config.get_org()?;
    let client = ApiClient::from_config(config)?;

    let provider_json = match provider {
        Some(p) => {
            // Check if it's already JSON (starts with { or [), otherwise wrap it in an object like {"id": "..."}
            let trimmed = p.trim();
            if trimmed.starts_with('{') || trimmed.starts_with('[') {
                Some(serde_json::from_str(trimmed).context("Invalid JSON in provider argument")?)
            } else {
                Some(serde_json::json!({ "id": trimmed, "name": trimmed }))
            }
        }
        None => None,
    };

    let req = CreateRepositoryRequest {
        name,
        url,
        provider: provider_json,
    };

    match client.create_repository(org, &req).await {
        Ok(_) => {
            println!("Created repository: {}", name);
            Ok(())
        }
        Err(e) => {
            let e_str = e.to_string();
            // Handle 409 Conflict if repository already exists
            if e_str.contains("409") {
                println!("Repository '{}' already exists", name);
                Ok(())
            } else {
                Err(e).context("Failed to create repository")
            }
        }
    }
}
