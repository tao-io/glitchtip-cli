use anyhow::{Context, Result};
use chrono::Utc;

use crate::api::ApiClient;
use crate::api::client::{Commit, NewRelease, UpdateRelease};
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
            .filter_map(|r| r.short_version.as_deref().or(r.version.as_deref()))
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
        let version = release.short_version.as_deref()
            .or(release.version.as_deref())
            .unwrap_or("?");
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

pub async fn delete(config: &Config, version: &str) -> Result<()> {
    let org = config
        .org
        .as_deref()
        .context("Organization not set. Use --org, SENTRY_ORG, or set it in .sentryclirc")?;

    let client = ApiClient::from_config(config)?;

    client
        .delete_release(org, version)
        .await
        .context("Failed to delete release")?;

    println!("Deleted release {version}.");

    Ok(())
}

pub async fn set_commits(
    config: &Config,
    version: &str,
    auto: bool,
    commits_arg: &[String],
) -> Result<()> {
    let org = config
        .org
        .as_deref()
        .context("Organization not set. Use --org, SENTRY_ORG, or set it in .sentryclirc")?;

    let client = ApiClient::from_config(config)?;

    let commits = if auto {
        discover_git_commits()?
    } else if !commits_arg.is_empty() {
        // Parse "COMMIT_SHA" or "REPO_NAME@COMMIT_SHA" format
        commits_arg
            .iter()
            .map(|c| Commit {
                id: c.clone(),
                message: None,
                author_name: None,
                author_email: None,
                timestamp: None,
            })
            .collect()
    } else {
        anyhow::bail!("Either --auto or --commit HASH must be specified");
    };

    if commits.is_empty() {
        println!("No commits found.");
        return Ok(());
    }

    println!("Setting {} commit(s) for release {version}...", commits.len());

    client
        .set_release_commits(org, version, &commits)
        .await
        .context("Failed to set commits")?;

    println!("Set commits for release {version}.");

    Ok(())
}

fn discover_git_commits() -> Result<Vec<Commit>> {
    let output = std::process::Command::new("git")
        .args(["log", "--format=%H%n%an%n%ae%n%s%n%aI%n---", "-20"])
        .output()
        .context("Failed to run git log. Is git installed and is this a git repository?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git log failed: {stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut commits = Vec::new();

    for entry in stdout.split("---\n") {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let lines: Vec<&str> = entry.lines().collect();
        if lines.len() >= 5 {
            commits.push(Commit {
                id: lines[0].to_string(),
                author_name: Some(lines[1].to_string()),
                author_email: Some(lines[2].to_string()),
                message: Some(lines[3].to_string()),
                timestamp: Some(lines[4].to_string()),
            });
        }
    }

    Ok(commits)
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
