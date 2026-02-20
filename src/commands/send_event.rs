use std::collections::HashMap;

use anyhow::Result;
use serde_json::json;

use crate::api::dsn::Dsn;
use crate::api::ApiClient;
use crate::config::Config;

pub struct SendEventOptions<'a> {
    pub message: &'a [String],
    pub level: &'a str,
    pub release: Option<&'a str>,
    pub environment: Option<&'a str>,
    pub tags: &'a [String],
    pub extra: &'a [String],
    pub platform: &'a str,
    pub fingerprint: &'a [String],
}

pub async fn execute(config: &Config, opts: &SendEventOptions<'_>) -> Result<()> {
    // 1. Get DSN
    let dsn_str = config
        .dsn
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No DSN configured. Set SENTRY_DSN or add dsn= under [auth] in .sentryclirc"
            )
        })?;

    // 2. Parse DSN
    let dsn = Dsn::parse(dsn_str)?;

    // 3. Build event JSON
    let event_id = uuid::Uuid::now_v7().to_string().replace('-', "");
    let timestamp = chrono::Utc::now().to_rfc3339();

    let message = if opts.message.is_empty() {
        "test event from glitchtip-cli".to_string()
    } else {
        opts.message.join("\n")
    };

    let tags = parse_key_value_pairs(opts.tags)?;
    let extra = parse_key_value_pairs(opts.extra)?;

    let mut event = json!({
        "event_id": event_id,
        "timestamp": timestamp,
        "level": opts.level,
        "platform": opts.platform,
        "logentry": { "message": message },
        "sdk": {
            "name": "glitchtip-cli",
            "version": env!("CARGO_PKG_VERSION"),
        },
    });

    let obj = event.as_object_mut().unwrap();

    if let Some(release) = opts.release {
        obj.insert("release".into(), json!(release));
    }
    if let Some(env) = opts.environment {
        obj.insert("environment".into(), json!(env));
    }
    if !tags.is_empty() {
        obj.insert("tags".into(), json!(tags));
    }
    if !extra.is_empty() {
        obj.insert("extra".into(), json!(extra));
    }
    if !opts.fingerprint.is_empty() {
        obj.insert("fingerprint".into(), json!(opts.fingerprint));
    }

    let event_json = serde_json::to_vec(&event)?;

    // 4. Build envelope
    let envelope = build_envelope(&event_id, dsn_str, &event_json);

    // 5. Send
    let client = ApiClient::from_config(config)?;
    client.send_envelope(&dsn, &envelope).await?;

    println!("{event_id}");
    Ok(())
}

/// Build a Sentry envelope from event JSON bytes.
pub fn build_envelope(event_id: &str, dsn: &str, event_json: &[u8]) -> Vec<u8> {
    let sent_at = chrono::Utc::now().to_rfc3339();

    let header = json!({
        "event_id": event_id,
        "dsn": dsn,
        "sent_at": sent_at,
    });

    let item_header = json!({
        "type": "event",
        "content_type": "application/json",
        "length": event_json.len(),
    });

    let mut envelope = Vec::new();
    envelope.extend_from_slice(header.to_string().as_bytes());
    envelope.push(b'\n');
    envelope.extend_from_slice(item_header.to_string().as_bytes());
    envelope.push(b'\n');
    envelope.extend_from_slice(event_json);
    envelope.push(b'\n');
    envelope
}

/// Parse `KEY:VALUE` pairs from CLI arguments into a map.
fn parse_key_value_pairs(pairs: &[String]) -> Result<HashMap<String, String>> {
    let mut map = HashMap::new();
    for pair in pairs {
        let (key, value) = pair
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("Invalid KEY:VALUE pair: {pair:?}"))?;
        map.insert(key.to_string(), value.to_string());
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_envelope_format() {
        let event_json = br#"{"event_id":"abc123","level":"error"}"#;
        let envelope = build_envelope("abc123", "https://key@sentry.io/1", event_json);
        let text = String::from_utf8(envelope).unwrap();

        let lines: Vec<&str> = text.split('\n').collect();
        // Should have: header, item header, event json, trailing empty after final newline
        assert_eq!(lines.len(), 4, "envelope should have 3 lines + trailing");

        // Parse and verify envelope header
        let header: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(header["event_id"], "abc123");
        assert_eq!(header["dsn"], "https://key@sentry.io/1");
        assert!(header["sent_at"].is_string());

        // Parse and verify item header
        let item_header: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(item_header["type"], "event");
        assert_eq!(item_header["content_type"], "application/json");
        assert_eq!(item_header["length"], event_json.len());

        // Verify event JSON is passed through verbatim
        assert_eq!(lines[2], std::str::from_utf8(event_json).unwrap());
    }

    #[test]
    fn test_parse_key_value_pairs() {
        let pairs = vec!["env:production".to_string(), "region:us-east".to_string()];
        let map = parse_key_value_pairs(&pairs).unwrap();
        assert_eq!(map["env"], "production");
        assert_eq!(map["region"], "us-east");
    }

    #[test]
    fn test_parse_key_value_pairs_with_colon_in_value() {
        let pairs = vec!["url:https://example.com".to_string()];
        let map = parse_key_value_pairs(&pairs).unwrap();
        assert_eq!(map["url"], "https://example.com");
    }

    #[test]
    fn test_parse_key_value_pairs_invalid() {
        let pairs = vec!["no-colon".to_string()];
        assert!(parse_key_value_pairs(&pairs).is_err());
    }

    #[test]
    fn test_parse_key_value_pairs_empty() {
        let map = parse_key_value_pairs(&[]).unwrap();
        assert!(map.is_empty());
    }
}
