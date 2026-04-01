use std::path::{Path, PathBuf};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::Principal;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DocMeta {
    pub id: String,
    pub filename: String,
    pub uploaded_at: String, // RFC3339
    pub page_count: Option<u32>,
    pub category: Option<String>,
    pub size_bytes: u64,
}

#[derive(Clone, Debug)]
pub struct LocalStorage {
    pub data_dir: PathBuf,
}

impl LocalStorage {
    /// Create a new LocalStorage rooted at `data_dir`, creating required dirs.
    pub fn new(data_dir: impl AsRef<Path>) -> Result<Self> {
        let data_dir = data_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&data_dir)
            .with_context(|| format!("creating data_dir {}", data_dir.display()))?;
        // Create top-level dir structure
        std::fs::create_dir_all(data_dir.join("anonymous").join("sessions"))?;
        std::fs::create_dir_all(data_dir.join("users"))?;
        std::fs::create_dir_all(data_dir.join("sessions"))?; // for session json files
        Ok(Self { data_dir })
    }

    /// Root directory for the given principal.
    pub fn root_for(&self, principal: &Principal) -> PathBuf {
        match principal {
            Principal::Anonymous(sid) => self
                .data_dir
                .join("anonymous")
                .join("sessions")
                .join(sid),
            Principal::User(uid) => self.data_dir.join("users").join(uid),
        }
    }

    /// Directory for a specific document.
    pub fn doc_dir(&self, principal: &Principal, doc_id: &str) -> PathBuf {
        self.root_for(principal).join("pdfs").join(doc_id)
    }

    /// Full path to the original PDF file.
    pub fn pdf_path(&self, principal: &Principal, doc_id: &str) -> PathBuf {
        self.doc_dir(principal, doc_id).join("original.pdf")
    }

    /// Save uploaded PDF bytes and write metadata.json. Returns DocMeta.
    pub fn save_pdf(
        &self,
        principal: &Principal,
        filename: &str,
        bytes: &[u8],
    ) -> Result<DocMeta> {
        let doc_id = uuid::Uuid::new_v4().to_string();
        let doc_dir = self.doc_dir(principal, &doc_id);
        std::fs::create_dir_all(&doc_dir)
            .with_context(|| format!("creating doc dir {}", doc_dir.display()))?;

        let pdf_path = doc_dir.join("original.pdf");
        std::fs::write(&pdf_path, bytes)
            .with_context(|| format!("writing pdf to {}", pdf_path.display()))?;

        let meta = DocMeta {
            id: doc_id.clone(),
            filename: filename.to_string(),
            uploaded_at: chrono::Utc::now().to_rfc3339(),
            page_count: None,
            category: None,
            size_bytes: bytes.len() as u64,
        };

        self.write_doc_meta(principal, &doc_id, &meta)?;
        Ok(meta)
    }

    /// Read metadata.json for a document.
    pub fn read_doc_meta(&self, principal: &Principal, doc_id: &str) -> Result<DocMeta> {
        let path = self.doc_dir(principal, doc_id).join("metadata.json");
        let contents = std::fs::read_to_string(&path)
            .with_context(|| format!("reading metadata {}", path.display()))?;
        let meta: DocMeta = serde_json::from_str(&contents)?;
        Ok(meta)
    }

    /// Write metadata.json for a document.
    pub fn write_doc_meta(
        &self,
        principal: &Principal,
        doc_id: &str,
        meta: &DocMeta,
    ) -> Result<()> {
        let path = self.doc_dir(principal, doc_id).join("metadata.json");
        let contents = serde_json::to_string_pretty(meta)?;
        std::fs::write(&path, contents)
            .with_context(|| format!("writing metadata {}", path.display()))?;
        Ok(())
    }

    /// List all documents for a principal.
    pub fn list_documents(&self, principal: &Principal) -> Result<Vec<DocMeta>> {
        let pdfs_dir = self.root_for(principal).join("pdfs");
        if !pdfs_dir.exists() {
            return Ok(vec![]);
        }
        let mut docs = Vec::new();
        for entry in std::fs::read_dir(&pdfs_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let doc_id = entry.file_name().to_string_lossy().to_string();
                match self.read_doc_meta(principal, &doc_id) {
                    Ok(meta) => docs.push(meta),
                    Err(e) => {
                        tracing::warn!("Failed to read metadata for doc {}: {}", doc_id, e);
                    }
                }
            }
        }
        // Sort by uploaded_at descending
        docs.sort_by(|a, b| b.uploaded_at.cmp(&a.uploaded_at));
        Ok(docs)
    }

    /// Delete a document directory.
    pub fn delete_document(&self, principal: &Principal, doc_id: &str) -> Result<()> {
        let doc_dir = self.doc_dir(principal, doc_id);
        if doc_dir.exists() {
            std::fs::remove_dir_all(&doc_dir)
                .with_context(|| format!("deleting doc dir {}", doc_dir.display()))?;
        }
        Ok(())
    }

    /// Read an arbitrary file within a document directory.
    pub fn read_doc_file(
        &self,
        principal: &Principal,
        doc_id: &str,
        filename: &str,
    ) -> Result<String> {
        let path = self.doc_dir(principal, doc_id).join(filename);
        std::fs::read_to_string(&path)
            .with_context(|| format!("reading doc file {}", path.display()))
    }

    /// Write an arbitrary file within a document directory.
    pub fn write_doc_file(
        &self,
        principal: &Principal,
        doc_id: &str,
        filename: &str,
        content: &str,
    ) -> Result<()> {
        let dir = self.doc_dir(principal, doc_id);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(filename);
        std::fs::write(&path, content)
            .with_context(|| format!("writing doc file {}", path.display()))
    }

    /// Read an arbitrary file relative to the user root.
    pub fn read_user_file(&self, principal: &Principal, relative_path: &str) -> Result<String> {
        let path = self.root_for(principal).join(relative_path);
        std::fs::read_to_string(&path)
            .with_context(|| format!("reading user file {}", path.display()))
    }

    /// Write an arbitrary file relative to the user root (creates parent dirs).
    pub fn write_user_file(
        &self,
        principal: &Principal,
        relative_path: &str,
        content: &str,
    ) -> Result<()> {
        let path = self.root_for(principal).join(relative_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, content)
            .with_context(|| format!("writing user file {}", path.display()))
    }

    /// List all sessions stored in data/sessions/ — returns (session_id, expires_at).
    pub fn list_sessions(&self) -> Result<Vec<(String, u64)>> {
        let sessions_dir = self.data_dir.join("sessions");
        if !sessions_dir.exists() {
            return Ok(vec![]);
        }
        let mut result = Vec::new();
        for entry in std::fs::read_dir(&sessions_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                if let Ok(contents) = std::fs::read_to_string(&path) {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&contents) {
                        let session_id = val["session_id"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string();
                        let expires_at = val["expires_at"].as_u64().unwrap_or(0);
                        if !session_id.is_empty() {
                            result.push((session_id, expires_at));
                        }
                    }
                }
            }
        }
        Ok(result)
    }
}
