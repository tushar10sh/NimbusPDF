use anyhow::{bail, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const DRIVE_API_BASE: &str = "https://www.googleapis.com/drive/v3";
const DRIVE_UPLOAD_BASE: &str = "https://www.googleapis.com/upload/drive/v3";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GdriveToken {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: Option<u64>,
    pub token_type: Option<String>,
    pub scope: Option<String>,
    /// Computed at save time: unix timestamp when token expires
    pub expires_at: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DriveFile {
    pub id: String,
    pub name: String,
    #[serde(rename = "mimeType")]
    pub mime_type: Option<String>,
    pub size: Option<String>,
}

pub struct GdriveClient {
    token: GdriveToken,
    token_path: PathBuf,
    client: Client,
}

impl GdriveClient {
    /// Load token from JSON file.
    pub fn new(token_path: impl AsRef<Path>) -> Result<Self> {
        let token_path = token_path.as_ref().to_path_buf();
        let contents = std::fs::read_to_string(&token_path)
            .with_context(|| format!("reading gdrive token from {}", token_path.display()))?;
        let token: GdriveToken = serde_json::from_str(&contents)?;
        Ok(Self {
            token,
            token_path,
            client: Client::new(),
        })
    }

    /// Refresh access token if it's expiring within 60 seconds.
    pub async fn refresh_token_if_needed(&mut self) -> Result<()> {
        let client_id = std::env::var("GOOGLE_CLIENT_ID")
            .context("GOOGLE_CLIENT_ID not set")?;
        let client_secret = std::env::var("GOOGLE_CLIENT_SECRET")
            .context("GOOGLE_CLIENT_SECRET not set")?;

        let needs_refresh = match self.token.expires_at {
            Some(exp) => chrono::Utc::now().timestamp() >= exp - 60,
            None => true,
        };

        if !needs_refresh {
            return Ok(());
        }

        let refresh_token = self
            .token
            .refresh_token
            .clone()
            .context("no refresh_token available")?;

        let params = [
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("refresh_token", refresh_token.as_str()),
            ("grant_type", "refresh_token"),
        ];

        let resp: GdriveToken = self
            .client
            .post("https://oauth2.googleapis.com/token")
            .form(&params)
            .send()
            .await?
            .json()
            .await?;

        let expires_at = chrono::Utc::now().timestamp()
            + resp.expires_in.unwrap_or(3600) as i64;

        self.token.access_token = resp.access_token;
        if resp.refresh_token.is_some() {
            self.token.refresh_token = resp.refresh_token;
        }
        self.token.expires_in = resp.expires_in;
        self.token.expires_at = Some(expires_at);

        // Persist updated token
        let contents = serde_json::to_string_pretty(&self.token)?;
        std::fs::write(&self.token_path, contents)?;

        Ok(())
    }

    /// Upload a file to Google Drive using multipart upload. Returns file ID.
    pub async fn upload_file(
        &mut self,
        local_path: &Path,
        filename: &str,
        mime_type: &str,
    ) -> Result<String> {
        self.refresh_token_if_needed().await?;

        let file_bytes = std::fs::read(local_path)
            .with_context(|| format!("reading {}", local_path.display()))?;

        // Build multipart request manually
        let metadata = serde_json::json!({ "name": filename });
        let metadata_json = serde_json::to_string(&metadata)?;

        let boundary = "nimbus_boundary_1a2b3c";
        let body = format!(
            "--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{metadata_json}\r\n--{boundary}\r\nContent-Type: {mime_type}\r\n\r\n",
            boundary = boundary,
            metadata_json = metadata_json,
            mime_type = mime_type,
        );

        let mut body_bytes = body.into_bytes();
        body_bytes.extend_from_slice(&file_bytes);
        body_bytes.extend_from_slice(format!("\r\n--{}--\r\n", boundary).as_bytes());

        let content_type = format!("multipart/related; boundary={}", boundary);

        let resp = self
            .client
            .post(format!("{}/files?uploadType=multipart", DRIVE_UPLOAD_BASE))
            .bearer_auth(&self.token.access_token)
            .header("Content-Type", content_type)
            .body(body_bytes)
            .send()
            .await?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            bail!("Drive upload failed: {}", text);
        }

        let file: serde_json::Value = resp.json().await?;
        let id = file["id"]
            .as_str()
            .context("no id in drive upload response")?
            .to_string();
        Ok(id)
    }

    /// Download a file from Google Drive to `dest_path`.
    pub async fn download_file(&mut self, file_id: &str, dest_path: &Path) -> Result<()> {
        self.refresh_token_if_needed().await?;

        let resp = self
            .client
            .get(format!("{}/files/{}?alt=media", DRIVE_API_BASE, file_id))
            .bearer_auth(&self.token.access_token)
            .send()
            .await?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            bail!("Drive download failed: {}", text);
        }

        let bytes = resp.bytes().await?;
        if let Some(parent) = dest_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(dest_path, &bytes)?;
        Ok(())
    }

    /// List files in Google Drive, optionally filtered by folder_id.
    pub async fn list_files(&mut self, folder_id: Option<&str>) -> Result<Vec<DriveFile>> {
        self.refresh_token_if_needed().await?;

        let mut url = format!("{}/files?fields=files(id,name,mimeType,size)", DRIVE_API_BASE);
        if let Some(fid) = folder_id {
            url.push_str(&format!("&q='{}' in parents", fid));
        }

        let resp: serde_json::Value = self
            .client
            .get(&url)
            .bearer_auth(&self.token.access_token)
            .send()
            .await?
            .json()
            .await?;

        let files = resp["files"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(|v| serde_json::from_value::<DriveFile>(v.clone()).ok())
            .collect();

        Ok(files)
    }
}
