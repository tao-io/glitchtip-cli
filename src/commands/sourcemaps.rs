use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::api::ApiClient;
use crate::api::client::AssembleRequest;
use crate::config::Config;

const DEFAULT_EXTENSIONS: &[&str] = &["js", "cjs", "mjs", "map", "jsbundle", "bundle"];
const SOURCE_EXTENSIONS: &[&str] = &["js", "cjs", "mjs", "jsbundle", "bundle"];

const CODE_SNIPPET_TEMPLATE: &str = r#"!function(){try{var e="undefined"!=typeof window?window:"undefined"!=typeof global?global:"undefined"!=typeof globalThis?globalThis:"undefined"!=typeof self?self:{},n=(new e.Error).stack;n&&(e._sentryDebugIds=e._sentryDebugIds||{},e._sentryDebugIds[n]="__SENTRY_DEBUG_ID__")}catch(e){}}();"#;
const DEBUGID_PLACEHOLDER: &str = "__SENTRY_DEBUG_ID__";
const DEBUGID_COMMENT_PREFIX: &str = "//# debugId";

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
    let org = config.get_org()?;

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
    // Upload chunk
    client.upload_chunk(upload_url, compressed, &checksum).await?;

    // Assemble into release
    let assemble = AssembleRequest {
        checksum: checksum.clone(),
        chunks: vec![checksum],
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

use crate::utils::{gzip_compress, sha1_digest, sha1_hex};

fn resolve_projects(config: &Config, cli_projects: &[String]) -> Result<Vec<String>> {
    if !cli_projects.is_empty() {
        return Ok(cli_projects.to_vec());
    }
    if let Some(ref project) = config.project {
        return Ok(vec![project.clone()]);
    }
    anyhow::bail!("No project specified. Use --project, SENTRY_PROJECT, or set it in .sentryclirc")
}

// --- sourcemaps inject helpers ---

fn debug_id_from_bytes_hashed(bytes: &[u8]) -> uuid::Uuid {
    let hash = sha1_digest(bytes);
    let mut sha1_bytes = [0u8; 16];
    sha1_bytes.copy_from_slice(&hash[..16]);
    uuid::Builder::from_sha1_bytes(sha1_bytes).into_uuid()
}

fn extract_sourcemap_url(source: &str) -> Option<&str> {
    source.lines().rev().find_map(|line| {
        let url = line
            .strip_prefix("//# sourceMappingURL=")
            .or_else(|| line.strip_prefix("//@ sourceMappingURL="))?;
        // Strip query string and fragment
        let url = url.split('?').next().unwrap_or(url);
        let url = url.split('#').next().unwrap_or(url);
        let url = url.trim();
        // Skip data: and remote URLs
        if url.is_empty()
            || url.starts_with("data:")
            || url.starts_with("http://")
            || url.starts_with("https://")
        {
            return None;
        }
        Some(url)
    })
}

fn resolve_sourcemap_path(source_path: &Path, url: &str) -> PathBuf {
    source_path.parent().unwrap_or(Path::new(".")).join(url)
}

fn has_debug_id(source: &str) -> bool {
    source.lines().any(|line| line.starts_with("//# debugId="))
}

fn extract_use_directive(source: &str) -> &str {
    let bytes = source.as_bytes();
    let mut pos = 0;

    // Skip leading whitespace and comments
    loop {
        while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if pos >= bytes.len() {
            return "";
        }
        if pos + 1 < bytes.len() && bytes[pos] == b'/' && bytes[pos + 1] == b'/' {
            // Line comment
            pos += 2;
            while pos < bytes.len() && bytes[pos] != b'\n' && bytes[pos] != b'\r' {
                pos += 1;
            }
            continue;
        }
        if pos + 1 < bytes.len() && bytes[pos] == b'/' && bytes[pos + 1] == b'*' {
            // Block comment
            pos += 2;
            loop {
                if pos + 1 >= bytes.len() {
                    return "";
                }
                if bytes[pos] == b'*' && bytes[pos + 1] == b'/' {
                    pos += 2;
                    break;
                }
                pos += 1;
            }
            continue;
        }
        break;
    }

    // Check for use directive: "use ..." or 'use ...'
    if pos < bytes.len() && (bytes[pos] == b'"' || bytes[pos] == b'\'') {
        let quote = bytes[pos];
        let start = pos;
        pos += 1;
        while pos < bytes.len() && bytes[pos] != quote {
            pos += 1;
        }
        if pos < bytes.len() {
            pos += 1; // include closing quote
            let candidate = &source[start..pos];
            let inner = &candidate[1..candidate.len() - 1];
            if inner.starts_with("use ") {
                return candidate;
            }
        }
    }

    ""
}

fn inject_at_start(source: &str, debug_id: uuid::Uuid) -> String {
    // Split off hashbang if present
    let (hashbang, rest) = if source.starts_with("#!") {
        match source.find('\n') {
            Some(i) => (&source[..i + 1], &source[i + 1..]),
            None => (source, ""),
        }
    } else {
        ("", source)
    };

    let use_directive = extract_use_directive(rest);
    let use_prefix = if use_directive.is_empty() {
        String::new()
    } else {
        format!("{use_directive};")
    };

    let snippet = CODE_SNIPPET_TEMPLATE.replace(DEBUGID_PLACEHOLDER, &debug_id.to_string());
    let debug_id_comment = format!("{DEBUGID_COMMENT_PREFIX}={debug_id}");

    format!("{hashbang}{use_prefix}{snippet}\n{rest}\n{debug_id_comment}\n")
}

fn inject_at_end(source: &str, debug_id: uuid::Uuid) -> String {
    let mut lines: Vec<&str> = source.lines().collect();

    // Find and remove the last sourceMappingURL comment
    let sourcemap_comment_idx = lines.iter().rposition(|line| {
        line.starts_with("//# sourceMappingURL=") || line.starts_with("//@ sourceMappingURL=")
    });
    let sourcemap_comment = sourcemap_comment_idx.map(|idx| lines.remove(idx));

    let snippet = CODE_SNIPPET_TEMPLATE.replace(DEBUGID_PLACEHOLDER, &debug_id.to_string());
    let debug_id_comment = format!("{DEBUGID_COMMENT_PREFIX}={debug_id}");

    let mut result = String::new();
    for line in &lines {
        result.push_str(line);
        result.push('\n');
    }
    result.push_str(&snippet);
    result.push('\n');
    if let Some(comment) = sourcemap_comment {
        result.push_str(comment);
        result.push('\n');
    }
    result.push_str(&debug_id_comment);
    result.push('\n');

    result
}

fn inject_sourcemap_debug_id(map_content: &str, debug_id: uuid::Uuid) -> Result<String> {
    let mut map: serde_json::Value =
        serde_json::from_str(map_content).context("Failed to parse sourcemap JSON")?;

    // Prepend ";" to mappings to shift all lines by 1
    if let Some(serde_json::Value::String(mappings)) = map.get_mut("mappings") {
        mappings.insert(0, ';');
    }

    // Add debug_id field
    map["debug_id"] = serde_json::Value::String(debug_id.to_string());

    serde_json::to_string(&map).context("Failed to serialize sourcemap JSON")
}

pub fn inject(
    paths: &[PathBuf],
    extensions: &[String],
    ignore: &[String],
    dry_run: bool,
) -> Result<()> {
    let exts: Vec<&str> = if extensions.is_empty() {
        DEFAULT_EXTENSIONS.to_vec()
    } else {
        // Always include .map for sourcemap discovery
        let mut e: Vec<&str> = extensions.iter().map(|s| s.as_str()).collect();
        if !e.iter().any(|ext| ext.eq_ignore_ascii_case("map")) {
            e.push("map");
        }
        e
    };

    let files = discover_files(paths, &exts, ignore)?;

    if files.is_empty() {
        println!("No files found.");
        return Ok(());
    }

    let source_files: Vec<&PathBuf> = files
        .iter()
        .filter(|f| {
            f.extension().and_then(|e| e.to_str()).is_some_and(|ext| {
                SOURCE_EXTENSIONS
                    .iter()
                    .any(|e| e.eq_ignore_ascii_case(ext))
            })
        })
        .collect();

    println!("Found {} JavaScript file(s)", source_files.len());

    let mut injected = 0;
    let mut already_has_debug_id = 0;

    for source_path in &source_files {
        let source = std::fs::read_to_string(source_path)
            .with_context(|| format!("Failed to read {}", source_path.display()))?;

        if has_debug_id(&source) {
            already_has_debug_id += 1;
            continue;
        }

        // Try to find corresponding sourcemap
        let sourcemap_path = extract_sourcemap_url(&source)
            .map(|url| resolve_sourcemap_path(source_path, url))
            .filter(|p| p.is_file())
            .or_else(|| {
                // Fallback: try <file>.map
                let ext = source_path
                    .extension()
                    .unwrap_or_default()
                    .to_string_lossy();
                let map_path = source_path.with_extension(format!("{ext}.map"));
                if map_path.is_file() {
                    Some(map_path)
                } else {
                    None
                }
            });

        if let Some(ref map_path) = sourcemap_path {
            // Has sourcemap: hash sourcemap → debug ID, inject at start
            let map_bytes = std::fs::read(map_path)
                .with_context(|| format!("Failed to read {}", map_path.display()))?;
            let debug_id = debug_id_from_bytes_hashed(&map_bytes);
            let map_content = String::from_utf8(map_bytes)
                .with_context(|| format!("Invalid UTF-8 in {}", map_path.display()))?;

            let new_source = inject_at_start(&source, debug_id);
            let new_map = inject_sourcemap_debug_id(&map_content, debug_id)?;

            if dry_run {
                println!(
                    "  [dry-run] {} (debug_id: {}, sourcemap: {})",
                    source_path.display(),
                    debug_id,
                    map_path.display()
                );
            } else {
                std::fs::write(source_path, &new_source)
                    .with_context(|| format!("Failed to write {}", source_path.display()))?;
                std::fs::write(map_path, &new_map)
                    .with_context(|| format!("Failed to write {}", map_path.display()))?;
            }
        } else {
            // No sourcemap: hash source → debug ID, inject at end
            let debug_id = debug_id_from_bytes_hashed(source.as_bytes());

            let new_source = inject_at_end(&source, debug_id);

            if dry_run {
                println!(
                    "  [dry-run] {} (debug_id: {}, no sourcemap)",
                    source_path.display(),
                    debug_id
                );
            } else {
                std::fs::write(source_path, &new_source)
                    .with_context(|| format!("Failed to write {}", source_path.display()))?;
            }
        }

        injected += 1;
    }

    println!();
    println!("Source map injection complete.");
    println!("  Injected: {injected} file(s)");
    if already_has_debug_id > 0 {
        println!("  Skipped (already injected): {already_has_debug_id} file(s)");
    }

    Ok(())
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

    // --- inject helper tests ---

    #[test]
    fn test_debug_id_from_bytes_hashed_deterministic() {
        let id1 = debug_id_from_bytes_hashed(b"test");
        let id2 = debug_id_from_bytes_hashed(b"test");
        assert_eq!(id1, id2);
        // SHA1("test") = a94a8fe5ccb19ba61c4c0873d391e987...
        // First 16 bytes with UUID v5 version/variant bits applied
        assert_eq!(id1.to_string(), "a94a8fe5-ccb1-5ba6-9c4c-0873d391e987");
    }

    #[test]
    fn test_debug_id_from_bytes_hashed_different_inputs() {
        let id1 = debug_id_from_bytes_hashed(b"hello");
        let id2 = debug_id_from_bytes_hashed(b"world");
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_extract_sourcemap_url_basic() {
        let source = "var x = 1;\n//# sourceMappingURL=app.js.map\n";
        assert_eq!(extract_sourcemap_url(source), Some("app.js.map"));
    }

    #[test]
    fn test_extract_sourcemap_url_query_string() {
        let source = "var x = 1;\n//# sourceMappingURL=app.js.map?v=123\n";
        assert_eq!(extract_sourcemap_url(source), Some("app.js.map"));
    }

    #[test]
    fn test_extract_sourcemap_url_fragment() {
        let source = "var x = 1;\n//# sourceMappingURL=app.js.map#hash\n";
        assert_eq!(extract_sourcemap_url(source), Some("app.js.map"));
    }

    #[test]
    fn test_extract_sourcemap_url_data_uri() {
        let source = "var x = 1;\n//# sourceMappingURL=data:application/json;base64,abc\n";
        assert_eq!(extract_sourcemap_url(source), None);
    }

    #[test]
    fn test_extract_sourcemap_url_remote() {
        let source = "var x = 1;\n//# sourceMappingURL=https://example.com/app.js.map\n";
        assert_eq!(extract_sourcemap_url(source), None);
    }

    #[test]
    fn test_extract_sourcemap_url_last_wins() {
        let source =
            "//# sourceMappingURL=first.map\nvar x = 1;\n//# sourceMappingURL=second.map\n";
        assert_eq!(extract_sourcemap_url(source), Some("second.map"));
    }

    #[test]
    fn test_extract_sourcemap_url_legacy_prefix() {
        let source = "var x = 1;\n//@ sourceMappingURL=app.js.map\n";
        assert_eq!(extract_sourcemap_url(source), Some("app.js.map"));
    }

    #[test]
    fn test_extract_sourcemap_url_none() {
        let source = "var x = 1;\n";
        assert_eq!(extract_sourcemap_url(source), None);
    }

    #[test]
    fn test_has_debug_id_true() {
        let source = "var x = 1;\n//# debugId=abc-123\n";
        assert!(has_debug_id(source));
    }

    #[test]
    fn test_has_debug_id_false() {
        let source = "var x = 1;\n";
        assert!(!has_debug_id(source));
    }

    #[test]
    fn test_extract_use_directive_simple() {
        assert_eq!(
            extract_use_directive("\"use strict\";\nvar x;"),
            "\"use strict\""
        );
    }

    #[test]
    fn test_extract_use_directive_single_quotes() {
        assert_eq!(
            extract_use_directive("'use strict';\nvar x;"),
            "'use strict'"
        );
    }

    #[test]
    fn test_extract_use_directive_with_leading_comment() {
        assert_eq!(
            extract_use_directive("// comment\n\"use strict\";\nvar x;"),
            "\"use strict\""
        );
    }

    #[test]
    fn test_extract_use_directive_with_block_comment() {
        assert_eq!(
            extract_use_directive("/* block */\"use strict\";\nvar x;"),
            "\"use strict\""
        );
    }

    #[test]
    fn test_extract_use_directive_none() {
        assert_eq!(extract_use_directive("var x = 1;"), "");
    }

    #[test]
    fn test_extract_use_directive_not_use() {
        assert_eq!(extract_use_directive("\"hello world\";\nvar x;"), "");
    }

    #[test]
    fn test_inject_at_start_simple() {
        let debug_id: uuid::Uuid = "a94a8fe5-ccb1-5ba6-9c4c-0873d391e987".parse().unwrap();
        let source = "var x = 1;\n";
        let result = inject_at_start(source, debug_id);

        assert!(result.starts_with("!function()"));
        assert!(result.contains("a94a8fe5-ccb1-5ba6-9c4c-0873d391e987"));
        assert!(result.contains("var x = 1;"));
        assert!(result.contains("//# debugId=a94a8fe5-ccb1-5ba6-9c4c-0873d391e987"));
    }

    #[test]
    fn test_inject_at_start_hashbang() {
        let debug_id: uuid::Uuid = "a94a8fe5-ccb1-5ba6-9c4c-0873d391e987".parse().unwrap();
        let source = "#!/usr/bin/node\nvar x = 1;\n";
        let result = inject_at_start(source, debug_id);

        assert!(result.starts_with("#!/usr/bin/node\n"));
        // Snippet comes after hashbang
        let after_hashbang = &result["#!/usr/bin/node\n".len()..];
        assert!(after_hashbang.starts_with("!function()"));
    }

    #[test]
    fn test_inject_at_start_use_strict() {
        let debug_id: uuid::Uuid = "a94a8fe5-ccb1-5ba6-9c4c-0873d391e987".parse().unwrap();
        let source = "\"use strict\";\nvar x = 1;\n";
        let result = inject_at_start(source, debug_id);

        // Use directive should be prepended to snippet
        assert!(result.starts_with("\"use strict\";!function()"));
    }

    #[test]
    fn test_inject_at_end_simple() {
        let debug_id: uuid::Uuid = "a94a8fe5-ccb1-5ba6-9c4c-0873d391e987".parse().unwrap();
        let source = "var x = 1;\n";
        let result = inject_at_end(source, debug_id);

        assert!(result.starts_with("var x = 1;\n"));
        assert!(result.contains("!function()"));
        assert!(result.ends_with("//# debugId=a94a8fe5-ccb1-5ba6-9c4c-0873d391e987\n"));
    }

    #[test]
    fn test_inject_at_end_moves_sourcemap_url() {
        let debug_id: uuid::Uuid = "a94a8fe5-ccb1-5ba6-9c4c-0873d391e987".parse().unwrap();
        let source = "var x = 1;\n//# sourceMappingURL=app.js.map\n";
        let result = inject_at_end(source, debug_id);

        // sourceMappingURL should be after the snippet, before debugId
        let lines: Vec<&str> = result.lines().collect();
        let sm_idx = lines
            .iter()
            .position(|l| l.starts_with("//# sourceMappingURL="))
            .unwrap();
        let dbg_idx = lines
            .iter()
            .position(|l| l.starts_with("//# debugId="))
            .unwrap();
        assert!(sm_idx < dbg_idx);
        // The snippet should be before the sourceMappingURL
        let snippet_idx = lines
            .iter()
            .position(|l| l.starts_with("!function()"))
            .unwrap();
        assert!(snippet_idx < sm_idx);
    }

    #[test]
    fn test_inject_sourcemap_debug_id() {
        let debug_id: uuid::Uuid = "a94a8fe5-ccb1-5ba6-9c4c-0873d391e987".parse().unwrap();
        let map = r#"{"version":3,"sources":[],"mappings":"AAAA"}"#;
        let result = inject_sourcemap_debug_id(map, debug_id).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

        // Mappings should have ";" prepended
        assert_eq!(parsed["mappings"].as_str().unwrap(), ";AAAA");
        // debug_id should be set
        assert_eq!(
            parsed["debug_id"].as_str().unwrap(),
            "a94a8fe5-ccb1-5ba6-9c4c-0873d391e987"
        );
    }
}
