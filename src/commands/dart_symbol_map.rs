use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};

use crate::api::ApiClient;
use crate::api::client::{DifAssembleFile, DifAssembleRequest};
use crate::config::Config;
use crate::utils::dif::DifFile;
use crate::utils::{gzip_compress, sha1_hex};

pub async fn upload(config: &Config, mapping: &Path, debug_file: &Path, wait: bool) -> Result<()> {
    let org = config.get_org()?;
    let project = config.project.as_deref().context(
        "No project specified. Use --project, SENTRY_PROJECT, or set it in .sentryclirc",
    )?;

    // Open the debug file and extract exactly one non-nil Debug ID
    let dif = DifFile::open_path(debug_file, None)
        .with_context(|| format!("Failed to open debug file {}", debug_file.display()))?;

    let ids: Vec<_> = dif.ids().filter(|id| !id.is_nil()).collect();
    if ids.is_empty() {
        anyhow::bail!(
            "No debug ID found in {}. The file may be stripped.",
            debug_file.display()
        );
    }
    if ids.len() > 1 {
        anyhow::bail!(
            "Found {} debug IDs in {}; expected exactly 1. Use a single-architecture binary.",
            ids.len(),
            debug_file.display()
        );
    }
    let debug_id = ids[0].to_string();
    println!("Debug ID: {debug_id}");

    // Read and validate the mapping file
    let data = std::fs::read(mapping)
        .with_context(|| format!("Failed to read mapping file {}", mapping.display()))?;

    let entries: Vec<String> = serde_json::from_slice(&data).with_context(|| {
        format!(
            "Failed to parse {} as JSON array of strings",
            mapping.display()
        )
    })?;

    if !entries.len().is_multiple_of(2) {
        anyhow::bail!(
            "Mapping file has {} entries; expected an even number (pairs of original/obfuscated names)",
            entries.len()
        );
    }
    println!(
        "Parsed {} symbol mapping pair(s) from {}",
        entries.len() / 2,
        mapping.display()
    );

    let client = ApiClient::from_config(config)?;

    // Get chunk upload info and validate file size
    let upload_info = client
        .get_chunk_upload_info(org)
        .await
        .context("Failed to get chunk upload info")?;

    if let Some(max_size) = upload_info.max_file_size
        && data.len() as u64 > max_size
    {
        anyhow::bail!(
            "Mapping file is {} bytes, which exceeds the server limit of {} bytes",
            data.len(),
            max_size
        );
    }

    // Compute checksums, compress, and upload
    let file_checksum = sha1_hex(&data);
    let compressed = gzip_compress(&data)?;
    let display_name = mapping
        .file_name()
        .unwrap_or(mapping.as_os_str())
        .to_string_lossy()
        .to_string();

    client
        .upload_chunk(&upload_info.url, compressed, &file_checksum)
        .await
        .context("Failed to upload mapping chunk")?;
    println!("Uploaded chunk for: {display_name}");

    // Assemble
    let mut files = HashMap::new();
    files.insert(
        file_checksum.clone(),
        DifAssembleFile {
            name: display_name.clone(),
            debug_id: Some(debug_id),
            chunks: vec![file_checksum.clone()],
        },
    );
    let request = DifAssembleRequest { files };

    println!("Requesting assembly...");
    let response = client.assemble_difs(org, project, &request).await?;

    if wait {
        let max_polls = 60;
        let poll_interval = std::time::Duration::from_secs(1);

        for _ in 0..max_polls {
            tokio::time::sleep(poll_interval).await;

            let response = client.assemble_difs(org, project, &request).await?;

            let mut all_done = true;
            for (checksum, status) in &response.files {
                let name = if checksum == &file_checksum {
                    &display_name
                } else {
                    checksum
                };
                match status.state.as_str() {
                    "ok" | "created" => {}
                    "error" => {
                        let detail = status.detail.as_deref().unwrap_or("unknown error");
                        anyhow::bail!("Assembly error for {name}: {detail}");
                    }
                    _ => {
                        all_done = false;
                    }
                }
            }

            if all_done {
                println!("Assembly completed.");
                return Ok(());
            }
        }

        println!("Assembly did not complete within timeout.");
    } else {
        for (checksum, status) in &response.files {
            let name = if checksum == &file_checksum {
                &display_name
            } else {
                checksum
            };
            match status.state.as_str() {
                "ok" | "created" => println!("  OK: {name}"),
                "error" => {
                    let detail = status.detail.as_deref().unwrap_or("unknown error");
                    anyhow::bail!("Assembly error for {name}: {detail}");
                }
                state => println!("  {name}: {state}"),
            }
        }
    }

    Ok(())
}
