use anyhow::{Context, Result, bail};
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

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

    // --- High-level API methods ---

    pub async fn get_auth_info(&self) -> Result<AuthInfo> {
        self.get("").await
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
}
