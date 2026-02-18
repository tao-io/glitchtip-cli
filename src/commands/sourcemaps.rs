use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::api::ApiClient;
use crate::api::client::AssembleRequest;
use crate::config::Config;

const DEFAULT_EXTENSIONS: &[&str] = &["js", "cjs", "mjs", "map", "jsbundle", "bundle"];

pub struct UploadOptions<'a> {
    pub paths: &'a [PathBuf],
    pub release: Option<&'a str>,
    pub projects: &'a [String],
    pub url_prefix: &'a str,
    #[allow(dead_code)] // Used for artifact bundle metadata
    pub dist: Option<&'a str>,
    pub extensions: &'a [String],
    pub ignore: &'a [String],
    pub validate: bool,
    pub no_rewrite: bool,
}

pub async fn upload(config: &Config, opts: &UploadOptions<'_>) -> Result<()> {
    let org = config
        .org
        .as_deref()
        .context("Organization not set. Use --org, SENTRY_ORG, or set it in .sentryclirc")?;

    let env_release = std::env::var("SENTRY_RELEASE").ok();
    let release_version = opts
        .release
        .or(env_release.as_deref())
        .context("Release not set. Use --release or SENTRY_RELEASE")?;

    let _projects = resolve_projects(config, opts.projects)?;
    let client = ApiClient::from_config(config)?;

    // Determine file extensions to use
    let exts: Vec<&str> = if opts.extensions.is_empty() {
        DEFAULT_EXTENSIONS.to_vec()
    } else {
        opts.extensions.iter().map(|s| s.as_str()).collect()
    };

    // Discover files
    let files = discover_files(opts.paths, &exts, opts.ignore)?;

    if files.is_empty() {
        println!("No files found to upload.");
        return Ok(());
    }

    println!("Found {} file(s) to upload.", files.len());

    if opts.validate {
        validate_sourcemaps(&files)?;
    }

    // Get chunk upload info
    let upload_info = client
        .get_chunk_upload_info(org)
        .await
        .context("Failed to get chunk upload info")?;

    let mut uploaded = 0;
    let mut errors = 0;

    for file_path in &files {
        let relative = file_path
            .strip_prefix(opts.paths.first().unwrap_or(&PathBuf::from(".")))
            .unwrap_or(file_path);

        let file_url = format!(
            "{}/{}",
            opts.url_prefix.trim_end_matches('/'),
            relative.to_string_lossy()
        );

        match upload_single_file(&client, &upload_info.url, org, release_version, file_path).await {
            Ok(()) => {
                println!("  Uploaded: {} -> {}", relative.display(), file_url);
                uploaded += 1;
            }
            Err(e) => {
                eprintln!("  Error uploading {}: {e}", relative.display());
                errors += 1;
            }
        }
    }

    println!();
    println!("Upload complete: {uploaded} file(s) uploaded, {errors} error(s).");

    if errors > 0 && !opts.no_rewrite {
        anyhow::bail!("{errors} file(s) failed to upload");
    }

    Ok(())
}

async fn upload_single_file(
    client: &ApiClient,
    upload_url: &str,
    org: &str,
    version: &str,
    file_path: &Path,
) -> Result<()> {
    // Read file
    let mut file = std::fs::File::open(file_path)
        .with_context(|| format!("Failed to open {}", file_path.display()))?;
    let mut data = Vec::new();
    file.read_to_end(&mut data)
        .with_context(|| format!("Failed to read {}", file_path.display()))?;

    // Compute SHA1 checksum
    let checksum = sha1_hex(&data);

    // Gzip compress
    let compressed = gzip_compress(&data)?;
    let chunk_checksum = sha1_hex(&compressed);

    // Upload chunk
    client.upload_chunk(upload_url, compressed).await?;

    // Assemble into release
    let assemble = AssembleRequest {
        checksum,
        chunks: vec![chunk_checksum],
    };

    client
        .assemble_release_files(org, version, &assemble)
        .await?;

    Ok(())
}

fn discover_files(
    paths: &[PathBuf],
    extensions: &[&str],
    ignore_patterns: &[String],
) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    for path in paths {
        if path.is_file() {
            if has_valid_extension(path, extensions) {
                files.push(path.clone());
            }
        } else if path.is_dir() {
            walk_dir(path, extensions, ignore_patterns, &mut files)?;
        } else {
            eprintln!("Warning: {} does not exist, skipping", path.display());
        }
    }

    files.sort();
    Ok(files)
}

fn walk_dir(
    dir: &Path,
    extensions: &[&str],
    ignore_patterns: &[String],
    files: &mut Vec<PathBuf>,
) -> Result<()> {
    let entries = std::fs::read_dir(dir)
        .with_context(|| format!("Failed to read directory {}", dir.display()))?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        // Skip hidden directories
        if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with('.'))
        {
            continue;
        }

        // Check ignore patterns (simple substring match)
        let path_str = path.to_string_lossy();
        if ignore_patterns
            .iter()
            .any(|p| path_str.contains(p.as_str()))
        {
            continue;
        }

        if path.is_dir() {
            // Skip node_modules by default
            if path.file_name().is_some_and(|n| n == "node_modules") {
                continue;
            }
            walk_dir(&path, extensions, ignore_patterns, files)?;
        } else if has_valid_extension(&path, extensions) {
            files.push(path);
        }
    }

    Ok(())
}

fn has_valid_extension(path: &Path, extensions: &[&str]) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| extensions.iter().any(|e| e.eq_ignore_ascii_case(ext)))
}

fn validate_sourcemaps(files: &[PathBuf]) -> Result<()> {
    for file in files {
        if file.extension().and_then(|e| e.to_str()) == Some("map") {
            let content = std::fs::read_to_string(file)
                .with_context(|| format!("Failed to read {}", file.display()))?;
            if serde_json::from_str::<serde_json::Value>(&content).is_err() {
                eprintln!(
                    "Warning: {} does not appear to be valid JSON",
                    file.display()
                );
            }
        }
    }
    Ok(())
}

fn sha1_hex(data: &[u8]) -> String {
    use std::fmt::Write;
    let hash = sha1_digest(data);
    let mut hex = String::with_capacity(40);
    for byte in &hash {
        write!(hex, "{byte:02x}").expect("hex write");
    }
    hex
}

fn gzip_compress(data: &[u8]) -> Result<Vec<u8>> {
    use std::io::Write;
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(data)?;
    encoder.finish().context("gzip compression failed")
}

// Minimal SHA-1 implementation (FIPS 180-4)
fn sha1_digest(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];

    let bit_len = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for block in msg.chunks_exact(64) {
        let mut w = [0u32; 80];
        for (i, chunk) in block.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);

        for (i, &wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999u32),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1u32),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDCu32),
                _ => (b ^ c ^ d, 0xCA62C1D6u32),
            };

            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }

    let mut result = [0u8; 20];
    for (i, &val) in h.iter().enumerate() {
        result[i * 4..i * 4 + 4].copy_from_slice(&val.to_be_bytes());
    }
    result
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

    #[test]
    fn test_sha1_known_value() {
        let hash = sha1_hex(b"hello");
        assert_eq!(hash, "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d");
    }

    #[test]
    fn test_sha1_empty() {
        let hash = sha1_hex(b"");
        assert_eq!(hash, "da39a3ee5e6b4b0d3255bfef95601890afd80709");
    }

    #[test]
    fn test_has_valid_extension() {
        assert!(has_valid_extension(Path::new("app.js"), &["js", "map"]));
        assert!(has_valid_extension(Path::new("app.map"), &["js", "map"]));
        assert!(!has_valid_extension(Path::new("app.css"), &["js", "map"]));
        assert!(!has_valid_extension(Path::new("noext"), &["js", "map"]));
    }

    #[test]
    fn test_gzip_roundtrip() {
        let data = b"hello world";
        let compressed = gzip_compress(data).unwrap();
        assert_ne!(compressed, data);
        assert!(!compressed.is_empty());

        use std::io::Read;
        let mut decoder = flate2::read::GzDecoder::new(&compressed[..]);
        let mut decompressed = String::new();
        decoder.read_to_string(&mut decompressed).unwrap();
        assert_eq!(decompressed, "hello world");
    }
}
