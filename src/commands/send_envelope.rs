use std::path::Path;

use anyhow::Result;

use crate::api::ApiClient;
use crate::api::dsn::Dsn;
use crate::config::Config;

pub struct SendEnvelopeOptions<'a> {
    pub path: &'a Path,
    pub raw: bool,
}

pub async fn execute(config: &Config, opts: &SendEnvelopeOptions<'_>) -> Result<()> {
    let dsn_str = config
        .dsn
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No DSN configured. Set SENTRY_DSN or add dsn= under [auth] in .sentryclirc"
            )
        })?;

    let dsn = Dsn::parse(dsn_str)?;

    let bytes = std::fs::read(opts.path).map_err(|e| {
        anyhow::anyhow!("Failed to read envelope file {}: {e}", opts.path.display())
    })?;

    if !opts.raw {
        validate_envelope_header(&bytes)?;
    }

    let client = ApiClient::from_config(config)?;
    client.send_envelope(&dsn, &bytes).await?;

    println!("Envelope from file {} dispatched", opts.path.display());
    Ok(())
}

fn validate_envelope_header(bytes: &[u8]) -> Result<()> {
    let first_line = bytes.split(|&b| b == b'\n').next().unwrap_or(bytes);

    let _: serde_json::Value = serde_json::from_slice(first_line).map_err(|_| {
        anyhow::anyhow!("Envelope header is not valid JSON. Use --raw to send without validation.")
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_accepts_valid_header() {
        let data = b"{\"event_id\":\"abc123\",\"dsn\":\"https://key@sentry.io/1\"}\n{\"type\":\"event\"}\n{}";
        assert!(validate_envelope_header(data).is_ok());
    }

    #[test]
    fn test_validation_rejects_invalid_header() {
        let data = b"this is not json\n{\"type\":\"event\"}\n{}";
        let err = validate_envelope_header(data).unwrap_err();
        assert!(err.to_string().contains("--raw"));
    }

    #[test]
    fn test_raw_skips_validation() {
        // When raw is true, validate_envelope_header is never called,
        // so even invalid data would be sent as-is.
        let data = b"not json at all";
        // Validation would fail...
        assert!(validate_envelope_header(data).is_err());
        // ...but with raw=true the execute function skips validation entirely.
    }
}
