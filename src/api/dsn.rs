use anyhow::{Context, Result, bail};

/// Parsed Sentry DSN (Data Source Name).
///
/// Format: `https://<public_key>@<host>/<project_id>`
#[derive(Debug, Clone)]
pub struct Dsn {
    pub scheme: String,
    pub public_key: String,
    pub host: String,
    pub port: Option<u16>,
    pub project_id: String,
}

impl Dsn {
    /// Parse a DSN string into its components.
    pub fn parse(dsn_str: &str) -> Result<Self> {
        let url = reqwest::Url::parse(dsn_str).context("Invalid DSN: not a valid URL")?;

        let scheme = url.scheme().to_string();
        if scheme != "http" && scheme != "https" {
            bail!("Invalid DSN: scheme must be http or https, got '{scheme}'");
        }

        let public_key = url.username().to_string();
        if public_key.is_empty() {
            bail!("Invalid DSN: missing public key (username component)");
        }

        let host = url
            .host_str()
            .context("Invalid DSN: missing host")?
            .to_string();

        let port = url.port();

        // Project ID is the last non-empty path segment
        let project_id = url
            .path()
            .trim_matches('/')
            .rsplit('/')
            .next()
            .filter(|s| !s.is_empty())
            .context("Invalid DSN: missing project ID in path")?
            .to_string();

        Ok(Dsn {
            scheme,
            public_key,
            host,
            port,
            project_id,
        })
    }

    /// Build the envelope endpoint URL.
    pub fn envelope_url(&self) -> String {
        let host_port = match self.port {
            Some(port) => format!("{}:{}", self.host, port),
            None => self.host.clone(),
        };
        format!(
            "{}://{}/api/{}/envelope/",
            self.scheme, host_port, self.project_id
        )
    }

    /// Build the X-Sentry-Auth header value.
    pub fn auth_header(&self) -> String {
        format!(
            "Sentry sentry_key={}, sentry_version=7, sentry_client=glitchtip-cli/{}",
            self.public_key,
            env!("CARGO_PKG_VERSION")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_dsn() {
        let dsn = Dsn::parse("https://abc123@o.ingest.sentry.io/456").unwrap();
        assert_eq!(dsn.scheme, "https");
        assert_eq!(dsn.public_key, "abc123");
        assert_eq!(dsn.host, "o.ingest.sentry.io");
        assert_eq!(dsn.port, None);
        assert_eq!(dsn.project_id, "456");
    }

    #[test]
    fn test_parse_dsn_with_port() {
        let dsn = Dsn::parse("http://key@localhost:9000/2").unwrap();
        assert_eq!(dsn.scheme, "http");
        assert_eq!(dsn.public_key, "key");
        assert_eq!(dsn.host, "localhost");
        assert_eq!(dsn.port, Some(9000));
        assert_eq!(dsn.project_id, "2");
    }

    #[test]
    fn test_parse_invalid_dsn_no_key() {
        let err = Dsn::parse("https://sentry.io/1").unwrap_err();
        assert!(
            err.to_string().contains("missing public key"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_parse_invalid_dsn_no_project() {
        let err = Dsn::parse("https://key@sentry.io/").unwrap_err();
        assert!(
            err.to_string().contains("missing project ID"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_parse_invalid_dsn_bad_scheme() {
        let err = Dsn::parse("ftp://key@sentry.io/1").unwrap_err();
        assert!(
            err.to_string().contains("scheme must be http or https"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_parse_invalid_dsn_not_url() {
        assert!(Dsn::parse("not a url").is_err());
    }

    #[test]
    fn test_envelope_url() {
        let dsn = Dsn::parse("https://abc@glitchtip.example.com/42").unwrap();
        assert_eq!(
            dsn.envelope_url(),
            "https://glitchtip.example.com/api/42/envelope/"
        );
    }

    #[test]
    fn test_envelope_url_with_port() {
        let dsn = Dsn::parse("http://abc@localhost:8000/7").unwrap();
        assert_eq!(dsn.envelope_url(), "http://localhost:8000/api/7/envelope/");
    }

    #[test]
    fn test_auth_header() {
        let dsn = Dsn::parse("https://mykey@sentry.io/1").unwrap();
        let header = dsn.auth_header();
        assert!(header.starts_with("Sentry sentry_key=mykey,"));
        assert!(header.contains("sentry_version=7"));
        assert!(header.contains("sentry_client=glitchtip-cli/"));
    }
}
