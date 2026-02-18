use anyhow::{Context, Result};
use chrono::Utc;

use crate::api::ApiClient;
use crate::api::client::{NewRelease, UpdateRelease};
use crate::config::Config;

pub async fn new_release(
    config: &Config,
    version: &str,
    projects: &[String],
    url: Option<&str>,
    finalize: bool,
) -> Result<()> {
    let org = config
        .org
        .as_deref()
        .context("Organization not set. Use --org, SENTRY_ORG, or set it in .sentryclirc")?;

    let projects = resolve_projects(config, projects)?;
    let client = ApiClient::from_config(config)?;

    let now = Utc::now().to_rfc3339();
    let release = NewRelease {
        version: version.to_string(),
        projects,
        url: url.map(String::from),
        date_started: Some(now.clone()),
        date_released: if finalize { Some(now) } else { None },
    };

    client
        .create_release(org, &release)
        .await
        .context("Failed to create release")?;

    if finalize {
        println!("Created and finalized release {version}.");
    } else {
        println!("Created release {version}.");
    }

    Ok(())
}

pub async fn finalize(
    config: &Config,
    version: &str,
    projects: &[String],
    url: Option<&str>,
    released: Option<&str>,
) -> Result<()> {
    let org = config
        .org
        .as_deref()
        .context("Organization not set. Use --org, SENTRY_ORG, or set it in .sentryclirc")?;

    let projects = resolve_projects(config, projects).ok();
    let client = ApiClient::from_config(config)?;

    let date_released = Some(match released {
        Some(ts) => ts.to_string(),
        None => Utc::now().to_rfc3339(),
    });

    let update = UpdateRelease {
        projects,
        url: url.map(String::from),
        date_released,
    };

    client
        .update_release(org, version, &update)
        .await
        .context("Failed to finalize release")?;

    println!("Finalized release {version}.");

    Ok(())
}

pub async fn list(
    config: &Config,
    project: Option<&str>,
    show_projects: bool,
    raw: bool,
    delimiter: Option<&str>,
) -> Result<()> {
    let org = config
        .org
        .as_deref()
        .context("Organization not set. Use --org, SENTRY_ORG, or set it in .sentryclirc")?;

    let client = ApiClient::from_config(config)?;

    // Use the specified project, or fall back to config default
    let proj = project.or(config.project.as_deref());
    let releases = client
        .list_releases(org, proj)
        .await
        .context("Failed to list releases")?;

    if raw {
        let delim = delimiter.unwrap_or("\n");
        let versions: Vec<&str> = releases
            .iter()
            .filter_map(|r| r.version.as_deref())
            .collect();
        print!("{}", versions.join(delim));
        if delim == "\n" {
            println!();
        }
        return Ok(());
    }

    if releases.is_empty() {
        println!("No releases found.");
        return Ok(());
    }

    // Print table header
    if show_projects {
        println!(
            "{:<20} {:<30} {:<20} {:>10} {:>16}",
            "Released", "Version", "Projects", "New Events", "Last Event"
        );
    } else {
        println!(
            "{:<20} {:<30} {:>10} {:>16}",
            "Released", "Version", "New Events", "Last Event"
        );
    }

    for release in &releases {
        let version = release.version.as_deref().unwrap_or("?");
        let released = release.date_released.as_deref().unwrap_or("(unreleased)");
        let last_event = release.last_event.as_deref().unwrap_or("-");
        let projects_str: String = release
            .projects
            .iter()
            .map(|p| p.slug.as_str())
            .collect::<Vec<_>>()
            .join(", ");

        if show_projects {
            println!(
                "{:<20} {:<30} {:<20} {:>10} {:>16}",
                released, version, projects_str, release.new_groups, last_event
            );
        } else {
            println!(
                "{:<20} {:<30} {:>10} {:>16}",
                released, version, release.new_groups, last_event
            );
        }
    }

    Ok(())
}

fn resolve_projects(config: &Config, cli_projects: &[String]) -> Result<Vec<String>> {
    if !cli_projects.is_empty() {
        return Ok(cli_projects.to_vec());
    }
    if let Some(ref project) = config.project {
        return Ok(vec![project.clone()]);
    }
    anyhow::bail!("No project specified. Use --project, SENTRY_PROJECT, or set it in .sentryclirc")
}
