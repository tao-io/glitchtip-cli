use anyhow::{Context, Result};

use crate::api::ApiClient;
use crate::config::Config;

pub async fn list(config: &Config) -> Result<()> {
    let client = ApiClient::from_config(config)?;
    let mut orgs = client
        .list_organizations()
        .await
        .context("Failed to list organizations")?;

    if orgs.is_empty() {
        println!("No organizations found.");
        return Ok(());
    }

    // Sort by slug for consistent output
    orgs.sort_by(|a, b| a.slug.cmp(&b.slug));

    println!("{:<30} {:<30}", "Name", "Slug");
    for org in orgs {
        println!("{:<30} {:<30}", org.name, org.slug);
    }

    Ok(())
}
