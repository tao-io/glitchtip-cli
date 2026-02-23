use anyhow::{Context, Result};

use crate::api::ApiClient;
use crate::config::Config;

pub async fn list(config: &Config, show_user: bool, show_tags: bool) -> Result<()> {
    let org = config.get_org()?;
    let project = config.get_project()?;

    let client = ApiClient::from_config(config)?;
    let events = client
        .list_events(org, project)
        .await
        .context("Failed to list events")?;

    if events.is_empty() {
        println!("No events found.");
        return Ok(());
    }

    // Build header
    let mut header = format!("{:<36} {:<24} {:<50}", "Event ID", "Date", "Title");
    if show_user {
        header.push_str(&format!(" {:<20}", "User"));
    }
    if show_tags {
        header.push_str(&format!(" {}", "Tags"));
    }
    println!("{header}");

    for event in &events {
        let title = if event.title.len() > 48 {
            format!("{}...", &event.title[..45])
        } else {
            event.title.clone()
        };

        let mut row = format!(
            "{:<36} {:<24} {:<50}",
            event.event_id, event.date_created, title
        );

        if show_user {
            let user_str = event
                .user
                .as_ref()
                .map(|u| u.to_string())
                .unwrap_or_else(|| "-".to_string());
            row.push_str(&format!(" {:<20}", user_str));
        }

        if show_tags {
            let tags_str = event
                .tags
                .as_ref()
                .map(|tags| {
                    tags.iter()
                        .map(|t| t.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_else(|| "-".to_string());
            row.push_str(&format!(" {}", tags_str));
        }

        println!("{row}");
    }

    Ok(())
}
