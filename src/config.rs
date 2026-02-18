use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Local config filenames, searched upward from cwd (higher priority first)
const LOCAL_CONFIG_FILES: &[&str] = &[".glitchtip-cli.rc", ".sentryclirc"];

#[derive(Debug, Clone, Default)]
pub struct Config {
    pub auth_token: Option<String>,
    pub url: String,
    pub org: Option<String>,
    pub project: Option<String>,
    pub log_level: String,
}

/// Return global config paths in priority order (highest first).
fn global_config_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // 1. XDG: ~/.config/glitchtip-cli/config (our own, preferred)
    if let Some(config_dir) = dirs::config_dir() {
        paths.push(config_dir.join("glitchtip-cli").join("config"));
    }

    // 2. sentry-cli compat: ~/.config/sentry/sentrycli.ini
    if let Some(config_dir) = dirs::config_dir() {
        paths.push(config_dir.join("sentry").join("sentrycli.ini"));
    }

    // 3. Legacy fallback: ~/.sentryclirc
    if let Some(home) = dirs::home_dir() {
        paths.push(home.join(".sentryclirc"));
    }

    paths
}

impl Config {
    /// Load config with full precedence:
    /// CLI flags > env vars > local config > global config
    ///
    /// Global config search order:
    ///   ~/.config/glitchtip-cli/config > ~/.config/sentry/sentrycli.ini > ~/.sentryclirc
    ///
    /// Local config search order (upward from cwd):
    ///   .glitchtip-cli.rc > .sentryclirc
    pub fn load() -> Result<Self> {
        let mut config = Config {
            url: "https://sentry.io/".to_string(),
            log_level: "warn".to_string(),
            ..Default::default()
        };

        // Load global config (lowest-priority first so higher-priority overwrites)
        let global_paths = global_config_paths();
        for path in global_paths.iter().rev() {
            if path.exists() {
                config.merge_ini_file(path)?;
            }
        }

        // Load local config (lowest-priority first)
        for filename in LOCAL_CONFIG_FILES.iter().rev() {
            if let Some(local_path) = find_local_config(filename) {
                config.merge_ini_file(&local_path)?;
            }
        }

        // Env vars override config files
        if let Ok(val) = std::env::var("SENTRY_AUTH_TOKEN") {
            config.auth_token = Some(val);
        }
        if let Ok(val) = std::env::var("SENTRY_URL") {
            config.url = val;
        }
        if let Ok(val) = std::env::var("SENTRY_ORG") {
            config.org = Some(val);
        }
        if let Ok(val) = std::env::var("SENTRY_PROJECT") {
            config.project = Some(val);
        }
        if let Ok(val) = std::env::var("SENTRY_LOG_LEVEL") {
            config.log_level = val;
        }

        // Ensure URL has trailing slash
        if !config.url.ends_with('/') {
            config.url.push('/');
        }

        Ok(config)
    }

    /// Apply CLI flag overrides (called after clap parsing)
    pub fn apply_cli_overrides(
        &mut self,
        auth_token: Option<&str>,
        url: Option<&str>,
        org: Option<&str>,
        project: Option<&str>,
        log_level: Option<&str>,
    ) {
        if let Some(v) = auth_token {
            self.auth_token = Some(v.to_string());
        }
        if let Some(v) = url {
            self.url = if v.ends_with('/') {
                v.to_string()
            } else {
                format!("{v}/")
            };
        }
        if let Some(v) = org {
            self.org = Some(v.to_string());
        }
        if let Some(v) = project {
            self.project = Some(v.to_string());
        }
        if let Some(v) = log_level {
            self.log_level = v.to_string();
        }
    }

    fn merge_ini_file(&mut self, path: &Path) -> Result<()> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;
        let sections = parse_ini(&content);

        if let Some(auth) = sections.get("auth")
            && let Some(token) = auth.get("token")
        {
            self.auth_token = Some(token.clone());
        }
        if let Some(defaults) = sections.get("defaults") {
            if let Some(url) = defaults.get("url") {
                self.url = url.clone();
            }
            if let Some(org) = defaults.get("org") {
                self.org = Some(org.clone());
            }
            if let Some(project) = defaults.get("project") {
                self.project = Some(project.clone());
            }
        }
        if let Some(log) = sections.get("log")
            && let Some(level) = log.get("level")
        {
            self.log_level = level.clone();
        }

        Ok(())
    }

    /// Get the path where login should save the token.
    /// Global: ~/.config/glitchtip-cli/config
    /// Local: .sentryclirc (sentry-cli compat for existing CI scripts)
    pub fn config_path(global: bool) -> Result<PathBuf> {
        if global {
            let config_dir = dirs::config_dir().context("Could not determine config directory")?;
            Ok(config_dir.join("glitchtip-cli").join("config"))
        } else {
            Ok(PathBuf::from(".sentryclirc"))
        }
    }

    /// Save auth token to config file, creating parent dirs if needed.
    pub fn save_token(path: &Path, token: &str) -> Result<()> {
        // Create parent directories if they don't exist
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
            && !parent.exists()
        {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create {}", parent.display()))?;
        }

        let mut content = String::new();

        // Read existing content if file exists
        if path.exists() {
            content = fs::read_to_string(path)
                .with_context(|| format!("Failed to read {}", path.display()))?;
        }

        let mut sections = parse_ini(&content);
        let auth = sections.entry("auth".to_string()).or_default();
        auth.insert("token".to_string(), token.to_string());

        let output = write_ini(&sections);
        fs::write(path, output).with_context(|| format!("Failed to write {}", path.display()))?;

        // Set permissions to 0o600 on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        }

        Ok(())
    }
}

/// Search upward from cwd for a config file by name
fn find_local_config(filename: &str) -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let candidate = dir.join(filename);
        if candidate.exists() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Minimal INI parser: returns section -> key -> value
fn parse_ini(content: &str) -> HashMap<String, HashMap<String, String>> {
    let mut sections: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut current_section = String::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            current_section = line[1..line.len() - 1].trim().to_string();
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            sections
                .entry(current_section.clone())
                .or_default()
                .insert(key.trim().to_string(), value.trim().to_string());
        }
    }

    sections
}

/// Write sections back to INI format
fn write_ini(sections: &HashMap<String, HashMap<String, String>>) -> String {
    let mut output = String::new();

    // Sort sections for deterministic output
    let mut section_names: Vec<&String> = sections.keys().collect();
    section_names.sort();

    for section_name in section_names {
        let entries = &sections[section_name];
        if !section_name.is_empty() {
            output.push_str(&format!("[{section_name}]\n"));
        }
        let mut keys: Vec<&String> = entries.keys().collect();
        keys.sort();
        for key in keys {
            output.push_str(&format!("{}={}\n", key, entries[key]));
        }
        output.push('\n');
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ini() {
        let content = r#"
[auth]
token=sntrys_xxx

[defaults]
url=https://glitchtip.example.com
org=my-org
project=my-project
"#;
        let sections = parse_ini(content);
        assert_eq!(sections["auth"]["token"], "sntrys_xxx");
        assert_eq!(sections["defaults"]["url"], "https://glitchtip.example.com");
        assert_eq!(sections["defaults"]["org"], "my-org");
        assert_eq!(sections["defaults"]["project"], "my-project");
    }

    #[test]
    fn test_parse_ini_with_comments() {
        let content = r#"
# This is a comment
; So is this
[auth]
token = abc123
"#;
        let sections = parse_ini(content);
        assert_eq!(sections["auth"]["token"], "abc123");
    }

    #[test]
    fn test_write_ini_roundtrip() {
        let mut sections = HashMap::new();
        let mut auth = HashMap::new();
        auth.insert("token".to_string(), "abc123".to_string());
        sections.insert("auth".to_string(), auth);

        let output = write_ini(&sections);
        let parsed = parse_ini(&output);
        assert_eq!(parsed["auth"]["token"], "abc123");
    }

    #[test]
    fn test_config_url_trailing_slash() {
        let mut config = Config {
            url: "https://example.com".to_string(),
            ..Default::default()
        };
        config.apply_cli_overrides(None, Some("https://example.com"), None, None, None);
        assert_eq!(config.url, "https://example.com/");
    }

    #[test]
    fn test_global_config_path() {
        let path = Config::config_path(true).unwrap();
        let path_str = path.to_string_lossy();
        assert!(
            path_str.contains("glitchtip-cli"),
            "Global config should be under glitchtip-cli dir: {path_str}"
        );
        assert!(
            path_str.ends_with("config"),
            "Global config should be named 'config': {path_str}"
        );
    }

    #[test]
    fn test_local_config_path() {
        let path = Config::config_path(false).unwrap();
        assert_eq!(path, PathBuf::from(".sentryclirc"));
    }

    #[test]
    fn test_global_config_paths_order() {
        let paths = global_config_paths();
        // Should have at least the XDG path and legacy path
        assert!(paths.len() >= 2);
        // First should be our XDG path
        let first = paths[0].to_string_lossy();
        assert!(
            first.contains("glitchtip-cli"),
            "First global path should be glitchtip-cli XDG: {first}"
        );
        // Last should be legacy ~/.sentryclirc
        let last = paths.last().unwrap().to_string_lossy();
        assert!(
            last.ends_with(".sentryclirc"),
            "Last global path should be legacy: {last}"
        );
    }
}
