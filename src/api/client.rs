use anyhow::{Context, Result, bail};
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::api::dsn::Dsn;
use crate::config::Config;

#[derive(Debug, Clone)]
pub struct ApiClient {
    client: reqwest::Client,
    base_url: String,
    auth_token: Option<String>,
}

// --- Request/Response types ---

#[derive(Debug, Serialize)]
pub struct NewRelease {
    pub version: String,
    pub projects: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(rename = "dateStarted", skip_serializing_if = "Option::is_none")]
    pub date_started: Option<String>,
    #[serde(rename = "dateReleased", skip_serializing_if = "Option::is_none")]
    pub date_released: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UpdateRelease {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub projects: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(rename = "dateReleased", skip_serializing_if = "Option::is_none")]
    pub date_released: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct ReleaseInfo {
    pub version: Option<String>,
    #[serde(alias = "shortVersion")]
    pub short_version: Option<String>,
    #[serde(alias = "dateCreated")]
    pub date_created: Option<String>,
    #[serde(alias = "dateReleased")]
    pub date_released: Option<String>,
    #[serde(default)]
    pub projects: Vec<ProjectInfo>,
    #[serde(alias = "newGroups", default)]
    pub new_groups: u64,
    #[serde(alias = "lastEvent")]
    pub last_event: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct ProjectInfo {
    pub name: Option<String>,
    pub slug: String,
    pub platform: Option<String>,
    #[serde(alias = "dateCreated")]
    pub date_created: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct OrganizationInfo {
    pub name: String,
    pub slug: String,
    #[serde(alias = "dateCreated")]
    pub date_created: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct IssueInfo {
    pub id: String,
    pub title: String,
    #[serde(alias = "shortId")]
    pub short_id: Option<String>,
    pub status: Option<String>,
    pub level: Option<String>,
    #[serde(alias = "lastSeen")]
    pub last_seen: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct ReleaseCommitInfo {
    pub id: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct EventInfo {
    #[serde(alias = "eventID")]
    pub event_id: String,
    #[serde(default, alias = "dateCreated")]
    pub date_created: String,
    #[serde(default)]
    pub title: String,
    pub user: Option<EventUserInfo>,
    pub tags: Option<Vec<EventTagInfo>>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct EventUserInfo {
    pub id: Option<String>,
    pub username: Option<String>,
    pub email: Option<String>,
    pub ip_address: Option<String>,
}

impl std::fmt::Display for EventUserInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if let Some(ref username) = self.username {
            return write!(f, "{username}");
        }
        if let Some(ref email) = self.email {
            return write!(f, "{email}");
        }
        if let Some(ref id) = self.id {
            return write!(f, "ID:{id}");
        }
        if let Some(ref ip) = self.ip_address {
            return write!(f, "{ip}");
        }
        write!(f, "-")
    }
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct EventTagInfo {
    pub key: String,
    pub value: String,
}

impl std::fmt::Display for EventTagInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}: {}", self.key, self.value)
    }
}

#[derive(Debug, Deserialize)]
pub struct AuthInfo {
    pub user: Option<UserInfo>,
    pub auth: Option<AuthDetail>,
}

#[derive(Debug, Deserialize)]
pub struct UserInfo {
    pub email: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AuthDetail {
    #[serde(default)]
    pub scopes: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct ChunkUploadInfo {
    pub url: String,
    #[serde(alias = "chunkSize")]
    pub chunk_size: u64,
    #[serde(alias = "hashAlgorithm")]
    pub hash_algorithm: String,
    #[serde(default)]
    pub accept: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct AssembleRequest {
    pub checksum: String,
    pub chunks: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct AssembleResponse {
    pub state: String,
    #[serde(alias = "missingChunks", default)]
    pub missing_chunks: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct MonitorInfo {
    pub id: serde_json::Value,
    pub name: String,
    #[serde(alias = "monitorType")]
    pub monitor_type: String,
    pub url: Option<String>,
    #[serde(alias = "endpointId")]
    pub endpoint_id: Option<String>,
    pub interval: Option<u64>,
    #[serde(alias = "isUp")]
    pub is_up: Option<bool>,
    #[serde(alias = "heartbeatEndpoint")]
    pub heartbeat_endpoint: Option<String>,
    pub created: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct MonitorCheckInfo {
    #[serde(alias = "isUp")]
    pub is_up: Option<bool>,
    #[serde(alias = "startCheck")]
    pub start_check: Option<String>,
    pub reason: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct IssueChanges {
    #[serde(rename = "status")]
    pub new_status: String,
}

#[derive(Debug, Serialize)]
pub struct NewDeploy {
    pub environment: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(rename = "dateStarted", skip_serializing_if = "Option::is_none")]
    pub date_started: Option<String>,
    #[serde(rename = "dateFinished", skip_serializing_if = "Option::is_none")]
    pub date_finished: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct DeployInfo {
    pub id: Option<serde_json::Value>,
    pub environment: Option<String>,
    pub url: Option<String>,
    #[serde(alias = "dateStarted")]
    pub date_started: Option<String>,
    #[serde(alias = "dateFinished")]
    pub date_finished: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Commit {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(rename = "author_name", skip_serializing_if = "Option::is_none")]
    pub author_name: Option<String>,
    #[serde(rename = "author_email", skip_serializing_if = "Option::is_none")]
    pub author_email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

impl ApiClient {
    pub fn from_config(config: &Config) -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent(format!("glitchtip-cli/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            client,
            base_url: config.url.trim_end_matches('/').to_string(),
            auth_token: config.auth_token.clone(),
        })
    }

    fn default_headers(&self) -> Result<HeaderMap> {
        let mut headers = HeaderMap::new();
        if let Some(ref token) = self.auth_token {
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {token}")).context("Invalid auth token")?,
            );
        }
        Ok(headers)
    }

    fn api_url(&self, path: &str) -> String {
        format!("{}/api/0/{}", self.base_url, path.trim_start_matches('/'))
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = self.api_url(path);
        let headers = self.default_headers()?;
        let resp = self
            .client
            .get(&url)
            .headers(headers)
            .send()
            .await
            .with_context(|| format!("GET {url} failed"))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("GET {url} returned {status}: {body}");
        }

        resp.json::<T>()
            .await
            .with_context(|| format!("Failed to parse response from GET {url}"))
    }

    pub async fn post<B: Serialize, T: DeserializeOwned>(&self, path: &str, body: &B) -> Result<T> {
        let url = self.api_url(path);
        let headers = self.default_headers()?;
        let resp = self
            .client
            .post(&url)
            .headers(headers)
            .json(body)
            .send()
            .await
            .with_context(|| format!("POST {url} failed"))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("POST {url} returned {status}: {body}");
        }

        resp.json::<T>()
            .await
            .with_context(|| format!("Failed to parse response from POST {url}"))
    }

    pub async fn put<B: Serialize, T: DeserializeOwned>(&self, path: &str, body: &B) -> Result<T> {
        let url = self.api_url(path);
        let headers = self.default_headers()?;
        let resp = self
            .client
            .put(&url)
            .headers(headers)
            .json(body)
            .send()
            .await
            .with_context(|| format!("PUT {url} failed"))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("PUT {url} returned {status}: {body}");
        }

        resp.json::<T>()
            .await
            .with_context(|| format!("Failed to parse response from PUT {url}"))
    }

    pub async fn delete(&self, path: &str) -> Result<()> {
        let url = self.api_url(path);
        let headers = self.default_headers()?;
        let resp = self
            .client
            .delete(&url)
            .headers(headers)
            .send()
            .await
            .with_context(|| format!("DELETE {url} failed"))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("DELETE {url} returned {status}: {body}");
        }

        Ok(())
    }

    pub async fn post_unauthenticated<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let url = self.api_url(path);
        let resp = self
            .client
            .post(&url)
            .json(body)
            .send()
            .await
            .with_context(|| format!("POST {url} failed"))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("POST {url} returned {status}: {body}");
        }

        resp.json::<T>()
            .await
            .with_context(|| format!("Failed to parse response from POST {url}"))
    }

    pub async fn upload_chunk(&self, url: &str, data: Vec<u8>) -> Result<()> {
        let headers = self.default_headers()?;
        let part = reqwest::multipart::Part::bytes(data).file_name("chunk");
        let form = reqwest::multipart::Form::new().part("file_gzip", part);

        let resp = self
            .client
            .post(url)
            .headers(headers)
            .multipart(form)
            .send()
            .await
            .with_context(|| format!("Chunk upload to {url} failed"))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("Chunk upload to {url} returned {status}: {body}");
        }

        Ok(())
    }

    /// Send a Sentry envelope using DSN authentication.
    pub async fn send_envelope(&self, dsn: &Dsn, envelope: &[u8]) -> Result<()> {
        let url = dsn.envelope_url();
        let resp = self
            .client
            .post(&url)
            .header("X-Sentry-Auth", dsn.auth_header())
            .header("Content-Type", "application/x-sentry-envelope")
            .body(envelope.to_vec())
            .send()
            .await
            .with_context(|| format!("POST {url} failed"))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("POST {url} returned {status}: {body}");
        }

        Ok(())
    }

    // --- High-level API methods ---

    pub async fn get_auth_info(&self) -> Result<AuthInfo> {
        self.get("").await
    }

    pub async fn get_release(&self, org: &str, version: &str) -> Result<ReleaseInfo> {
        let encoded = urlencoding::encode(version);
        self.get(&format!("organizations/{org}/releases/{encoded}/"))
            .await
    }

    pub async fn get_release_commits(
        &self,
        org: &str,
        version: &str,
    ) -> Result<Vec<ReleaseCommitInfo>> {
        let encoded = urlencoding::encode(version);
        self.get(&format!("organizations/{org}/releases/{encoded}/commits/"))
            .await
    }

    pub async fn create_release(&self, org: &str, release: &NewRelease) -> Result<ReleaseInfo> {
        self.post(&format!("organizations/{org}/releases/"), release)
            .await
    }

    pub async fn update_release(
        &self,
        org: &str,
        version: &str,
        update: &UpdateRelease,
    ) -> Result<ReleaseInfo> {
        let encoded = urlencoding::encode(version);
        self.put(&format!("organizations/{org}/releases/{encoded}/"), update)
            .await
    }

    pub async fn list_releases(
        &self,
        org: &str,
        project: Option<&str>,
    ) -> Result<Vec<ReleaseInfo>> {
        let path = match project {
            Some(proj) => format!("organizations/{org}/releases/?project={proj}"),
            None => format!("organizations/{org}/releases/"),
        };
        self.get(&path).await
    }

    pub async fn get_chunk_upload_info(&self, org: &str) -> Result<ChunkUploadInfo> {
        self.get(&format!("organizations/{org}/chunk-upload/"))
            .await
    }

    pub async fn assemble_release_files(
        &self,
        org: &str,
        version: &str,
        request: &AssembleRequest,
    ) -> Result<AssembleResponse> {
        let encoded = urlencoding::encode(version);
        self.post(
            &format!("organizations/{org}/releases/{encoded}/assemble/"),
            request,
        )
        .await
    }

    pub async fn delete_release(&self, org: &str, version: &str) -> Result<()> {
        let encoded = urlencoding::encode(version);
        self.delete(&format!("organizations/{org}/releases/{encoded}/"))
            .await
    }

    pub async fn set_release_commits(
        &self,
        org: &str,
        version: &str,
        commits: &[Commit],
    ) -> Result<ReleaseInfo> {
        let encoded = urlencoding::encode(version);
        self.post(
            &format!("organizations/{org}/releases/{encoded}/commits/"),
            &commits,
        )
        .await
    }

    pub async fn create_deploy(
        &self,
        org: &str,
        version: &str,
        deploy: &NewDeploy,
    ) -> Result<DeployInfo> {
        let encoded = urlencoding::encode(version);
        self.post(
            &format!("organizations/{org}/releases/{encoded}/deploys/"),
            deploy,
        )
        .await
    }

    pub async fn list_deploys(&self, org: &str, version: &str) -> Result<Vec<DeployInfo>> {
        let encoded = urlencoding::encode(version);
        self.get(&format!("organizations/{org}/releases/{encoded}/deploys/"))
            .await
    }

    pub async fn list_organizations(&self) -> Result<Vec<OrganizationInfo>> {
        self.get("organizations/").await
    }

    pub async fn list_projects(&self, org: &str) -> Result<Vec<ProjectInfo>> {
        self.get(&format!("organizations/{org}/projects/")).await
    }

    pub async fn list_events(&self, org: &str, project: &str) -> Result<Vec<EventInfo>> {
        self.get(&format!("projects/{org}/{project}/events/")).await
    }

    pub async fn list_issues(&self, org: &str, project: &str) -> Result<Vec<IssueInfo>> {
        self.get(&format!("projects/{org}/{project}/issues/")).await
    }

    pub async fn list_monitors(&self, org: &str) -> Result<Vec<MonitorInfo>> {
        self.get(&format!("organizations/{org}/monitors/")).await
    }

    pub async fn heartbeat_check(
        &self,
        org: &str,
        endpoint_id: &str,
    ) -> Result<MonitorCheckInfo> {
        self.post_unauthenticated(
            &format!("organizations/{org}/heartbeat_check/{endpoint_id}/"),
            &serde_json::Value::Object(serde_json::Map::new()),
        )
        .await
    }

    pub async fn update_issues(
        &self,
        org: &str,
        project: &str,
        filter_query: &str,
        changes: &IssueChanges,
    ) -> Result<()> {
        let path = format!("projects/{org}/{project}/issues/?{filter_query}");
        let _: serde_json::Value = self.put(&path, changes).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Server;

    #[tokio::test]
    async fn test_list_organizations() {
        let mut server = Server::new_async().await;
        let url = server.url();

        let _m = server
            .mock("GET", "/api/0/organizations/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"[{"name": "My Org", "slug": "my-org", "dateCreated": "2023-01-01T00:00:00Z"}]"#,
            )
            .create_async()
            .await;

        let config = Config {
            url: url.clone(),
            auth_token: Some("token".to_string()),
            log_level: "warn".to_string(),
            ..Default::default()
        };

        let client = ApiClient::from_config(&config).unwrap();
        let orgs = client.list_organizations().await.unwrap();

        assert_eq!(orgs.len(), 1);
        assert_eq!(orgs[0].name, "My Org");
        assert_eq!(orgs[0].slug, "my-org");
    }

    #[tokio::test]
    async fn test_list_projects() {
        let mut server = Server::new_async().await;
        let url = server.url();

        let _m = server
            .mock("GET", "/api/0/organizations/my-org/projects/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"name": "My Project", "slug": "my-project", "platform": "javascript", "dateCreated": "2023-01-01T00:00:00Z"}]"#)
            .create_async()
            .await;

        let config = Config {
            url: url.clone(),
            auth_token: Some("token".to_string()),
            org: Some("my-org".to_string()),
            log_level: "warn".to_string(),
            ..Default::default()
        };

        let client = ApiClient::from_config(&config).unwrap();
        let projects = client.list_projects("my-org").await.unwrap();

        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].slug, "my-project");
        assert_eq!(projects[0].platform.as_deref(), Some("javascript"));
    }

    #[tokio::test]
    async fn test_update_issues() {
        let mut server = Server::new_async().await;
        let url = server.url();

        let _m = server
            .mock(
                "PUT",
                "/api/0/projects/my-org/my-project/issues/?id=42&id=43",
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"status": "resolved"}"#)
            .create_async()
            .await;

        let config = Config {
            url: url.clone(),
            auth_token: Some("token".to_string()),
            org: Some("my-org".to_string()),
            project: Some("my-project".to_string()),
            log_level: "warn".to_string(),
            ..Default::default()
        };

        let client = ApiClient::from_config(&config).unwrap();
        let changes = IssueChanges {
            new_status: "resolved".to_string(),
        };
        client
            .update_issues("my-org", "my-project", "id=42&id=43", &changes)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_list_issues() {
        let mut server = Server::new_async().await;
        let url = server.url();

        let _m = server
            .mock("GET", "/api/0/projects/my-org/my-project/issues/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"id": "42", "title": "TypeError: null is not an object", "shortId": "MY-PROJECT-1", "status": "unresolved", "level": "error", "lastSeen": "2024-01-15T10:30:00Z"}]"#)
            .create_async()
            .await;

        let config = Config {
            url: url.clone(),
            auth_token: Some("token".to_string()),
            org: Some("my-org".to_string()),
            project: Some("my-project".to_string()),
            log_level: "warn".to_string(),
            ..Default::default()
        };

        let client = ApiClient::from_config(&config).unwrap();
        let issues = client.list_issues("my-org", "my-project").await.unwrap();

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].id, "42");
        assert_eq!(issues[0].title, "TypeError: null is not an object");
        assert_eq!(issues[0].short_id.as_deref(), Some("MY-PROJECT-1"));
        assert_eq!(issues[0].status.as_deref(), Some("unresolved"));
        assert_eq!(issues[0].level.as_deref(), Some("error"));
    }

    #[tokio::test]
    async fn test_list_monitors() {
        let mut server = Server::new_async().await;
        let url = server.url();

        let _m = server
            .mock("GET", "/api/0/organizations/my-org/monitors/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"id": 1, "name": "My Website", "monitorType": "Ping", "url": "https://example.com", "endpointId": "abc-123", "interval": 60, "isUp": true, "heartbeatEndpoint": null, "created": "2024-01-01T00:00:00Z"}]"#)
            .create_async()
            .await;

        let config = Config {
            url: url.clone(),
            auth_token: Some("token".to_string()),
            org: Some("my-org".to_string()),
            log_level: "warn".to_string(),
            ..Default::default()
        };

        let client = ApiClient::from_config(&config).unwrap();
        let monitors = client.list_monitors("my-org").await.unwrap();

        assert_eq!(monitors.len(), 1);
        assert_eq!(monitors[0].name, "My Website");
        assert_eq!(monitors[0].monitor_type, "Ping");
        assert_eq!(monitors[0].url.as_deref(), Some("https://example.com"));
        assert_eq!(monitors[0].is_up, Some(true));
        assert_eq!(monitors[0].interval, Some(60));
    }

    #[tokio::test]
    async fn test_heartbeat_check() {
        let mut server = Server::new_async().await;
        let url = server.url();

        let endpoint_id = "550e8400-e29b-41d4-a716-446655440000";

        let _m = server
            .mock(
                "POST",
                &*format!(
                    "/api/0/organizations/my-org/heartbeat_check/{endpoint_id}/"
                ),
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"isUp": true, "startCheck": "2024-01-15T10:30:00Z", "reason": null}"#)
            .create_async()
            .await;

        let config = Config {
            url: url.clone(),
            auth_token: Some("token".to_string()),
            org: Some("my-org".to_string()),
            log_level: "warn".to_string(),
            ..Default::default()
        };

        let client = ApiClient::from_config(&config).unwrap();
        let check = client
            .heartbeat_check("my-org", endpoint_id)
            .await
            .unwrap();

        assert_eq!(check.is_up, Some(true));
        assert_eq!(
            check.start_check.as_deref(),
            Some("2024-01-15T10:30:00Z")
        );
        assert_eq!(check.reason, None);
    }
}
