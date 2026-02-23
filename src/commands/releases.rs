use anyhow::{Context, Result};
use chrono::{DateTime, Utc};

use crate::api::ApiClient;
use crate::api::client::{Commit, NewRelease, UpdateRelease};
use crate::config::Config;

pub async fn info(
    config: &Config,
    version: &str,
    show_projects: bool,
    show_commits: bool,
) -> Result<()> {
    let org = config
        .org
        .as_deref()
        .context("Organization not set. Use --org, SENTRY_ORG, or set it in .sentryclirc")?;

    let client = ApiClient::from_config(config)?;

    let release = client
        .get_release(org, version)
        .await
        .context("Failed to get release")?;

    let version_str = release
        .short_version
        .as_deref()
        .or(release.version.as_deref())
        .unwrap_or("?");
    let date_created = release.date_created.as_deref().unwrap_or("-");

    // Build header
    let mut header = format!("{:<30} {:<24}", "Version", "Date created");
    if release.last_event.is_some() {
        header.push_str(&format!(" {:<24}", "Last event"));
    }
    if show_projects {
        header.push_str(&format!(" {:<20}", "Projects"));
    }
    if show_commits {
        header.push_str(&format!(" {:<20}", "Commits"));
    }
    println!("{header}");

    // Build data row
    let mut row = format!("{:<30} {:<24}", version_str, date_created);
    if let Some(ref last_event) = release.last_event {
        row.push_str(&format!(" {:<24}", last_event));
    }
    if show_projects {
        let projects_str: String = release
            .projects
            .iter()
            .map(|p| p.slug.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        if projects_str.is_empty() {
            row.push_str(&format!(" {:<20}", "-"));
        } else {
            row.push_str(&format!(" {:<20}", projects_str));
        }
    }
    if show_commits {
        match client.get_release_commits(org, version).await {
            Ok(commits) if !commits.is_empty() => {
                let ids: Vec<&str> = commits.iter().map(|c| c.id.as_str()).collect();
                row.push_str(&format!(" {}", ids.join(", ")));
            }
            _ => {
                row.push_str(&format!(" {:<20}", "-"));
            }
        }
    }
    println!("{row}");

    Ok(())
}

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

    let now = DateTime::<Utc>::from(std::time::SystemTime::now()).to_rfc3339();
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
        None => DateTime::<Utc>::from(std::time::SystemTime::now()).to_rfc3339(),
    });

    let update = UpdateRelease {
        projects,
        url: url.map(String::from),
        date_released,
        status: None,
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
        let version = release
            .short_version
            .as_deref()
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

pub async fn archive(config: &Config, version: &str) -> Result<()> {
    let org = config
        .org
        .as_deref()
        .context("Organization not set. Use --org, SENTRY_ORG, or set it in .sentryclirc")?;

    let client = ApiClient::from_config(config)?;

    let update = UpdateRelease {
        projects: None,
        url: None,
        date_released: None,
        status: Some("archived".to_string()),
    };

    client
        .update_release(org, version, &update)
        .await
        .context("Failed to archive release")?;

    println!("Archived release {version}.");
    Ok(())
}

pub async fn restore(config: &Config, version: &str) -> Result<()> {
    let org = config
        .org
        .as_deref()
        .context("Organization not set. Use --org, SENTRY_ORG, or set it in .sentryclirc")?;

    let client = ApiClient::from_config(config)?;

    let update = UpdateRelease {
        projects: None,
        url: None,
        date_released: None,
        status: Some("open".to_string()),
    };

    client
        .update_release(org, version, &update)
        .await
        .context("Failed to restore release")?;

    println!("Restored release {version}.");
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

    println!(
        "Setting {} commit(s) for release {version}...",
        commits.len()
    );

    client
        .set_release_commits(org, version, &commits)
        .await
        .context("Failed to set commits")?;

    println!("Set commits for release {version}.");

    Ok(())
}

pub fn propose_version() -> Result<String> {
    let output = std::process::Command::new("git")
        .args(["describe", "--tags", "--always", "--dirty"])
        .output()
        .context("Failed to run git describe. Is git installed and is this a git repository?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git describe failed: {stderr}");
    }

    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(version)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use tempfile::tempdir;

    #[test]
    fn test_propose_version_git() {
        let dir = tempdir().unwrap();
        let repo_path = dir.path();

        // Initialize git repo
        Command::new("git")
            .arg("init")
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Configure git user
        Command::new("git")
            .args(["config", "user.email", "you@example.com"])
            .current_dir(repo_path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Your Name"])
            .current_dir(repo_path)
            .output()
            .unwrap();
        // Initial branch name can vary, so we force one
        Command::new("git")
            .args(["config", "init.defaultBranch", "main"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Create a file and commit it
        fs::write(repo_path.join("file.txt"), "hello").unwrap();
        Command::new("git")
            .args(["add", "file.txt"])
            .current_dir(repo_path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "initial commit"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Tag the commit
        Command::new("git")
            .args(["tag", "v1.2.3"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        let old_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(repo_path).unwrap();

        let version = propose_version().unwrap();
        assert_eq!(version, "v1.2.3");

        // Make it dirty
        fs::write(repo_path.join("file.txt"), "world").unwrap();
        let version = propose_version().unwrap();
        assert!(version.ends_with("-dirty"));

        std::env::set_current_dir(old_cwd).unwrap();
    }
}
