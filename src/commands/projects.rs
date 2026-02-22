use anyhow::{Context, Result};

use crate::api::ApiClient;
use crate::config::Config;

pub async fn list(config: &Config) -> Result<()> {
    let org = config
        .org
        .as_deref()
        .context("Organization not set. Use --org, SENTRY_ORG, or set it in .sentryclirc")?;

    let client = ApiClient::from_config(config)?;
    let mut projects = client
        .list_projects(org)
        .await
        .context("Failed to list projects")?;

    if projects.is_empty() {
        println!("No projects found.");
        return Ok(());
    }

    // Sort by slug for consistent output
    projects.sort_by(|a, b| a.slug.cmp(&b.slug));

    println!("{:<30} {:<30} {:<20}", "Name", "Slug", "Platform");
    for project in projects {
        let name = project.name.as_deref().unwrap_or(&project.slug);
        let platform = project.platform.as_deref().unwrap_or("-");
        println!("{:<30} {:<30} {:<20}", name, project.slug, platform);
    }

    Ok(())
}
