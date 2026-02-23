use anyhow::{Context, Result};
use chrono::{DateTime, Utc};

use crate::api::ApiClient;
use crate::api::client::NewDeploy;
use crate::config::Config;

pub async fn new_deploy(
    config: &Config,
    version: &str,
    environment: &str,
    url: Option<&str>,
    started: Option<&str>,
    finished: Option<&str>,
) -> Result<()> {
    let org = config.get_org()?;

    let client = ApiClient::from_config(config)?;

    let now: DateTime<Utc> = std::time::SystemTime::now().into();
    let now = now.to_rfc3339();
    let deploy = NewDeploy {
        environment: environment.to_string(),
        url: url.map(String::from),
        date_started: Some(started.unwrap_or(&now).to_string()),
        date_finished: Some(finished.unwrap_or(&now).to_string()),
    };

    client
        .create_deploy(org, version, &deploy)
        .await
        .context("Failed to create deploy")?;

    println!("Created deploy for release {version} in environment {environment}.");

    Ok(())
}

pub async fn list(config: &Config, version: &str) -> Result<()> {
    let org = config.get_org()?;

    let client = ApiClient::from_config(config)?;

    let deploys = client
        .list_deploys(org, version)
        .await
        .context("Failed to list deploys")?;

    if deploys.is_empty() {
        println!("No deploys found for release {version}.");
        return Ok(());
    }

    println!(
        "{:<20} {:<20} {:<24} {:<24}",
        "Environment", "URL", "Started", "Finished"
    );

    for deploy in &deploys {
        let env = deploy.environment.as_deref().unwrap_or("?");
        let url = deploy.url.as_deref().unwrap_or("");
        let started = deploy.date_started.as_deref().unwrap_or("-");
        let finished = deploy.date_finished.as_deref().unwrap_or("-");
        println!("{:<20} {:<20} {:<24} {:<24}", env, url, started, finished);
    }

    Ok(())
}
