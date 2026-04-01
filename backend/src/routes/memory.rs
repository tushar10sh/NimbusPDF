use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Extension, Json, Router,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::task::spawn_blocking;

use crate::ai::{AiProxy, Message, UserAiConfig};
use crate::session::SessionHandle;
use crate::storage::Principal;
use crate::AppState;

const MEMORY_FILE: &str = "memory/long_term_memory.md";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(get_memory).put(save_memory))
        .route("/append", post(append_document_memory))
}

#[derive(Deserialize, Serialize)]
pub struct MemoryDocument {
    pub content: String,
    pub updated_at: String,
}

#[derive(Deserialize)]
pub struct AppendMemoryRequest {
    pub doc_id: String,
}

fn require_auth(handle: &SessionHandle) -> Result<Principal, StatusCode> {
    if !handle.data.is_authenticated() {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(Principal::from_session(&handle.data))
}

async fn get_memory(
    State(state): State<AppState>,
    Extension(handle): Extension<SessionHandle>,
) -> Result<Json<MemoryDocument>, StatusCode> {
    let principal = require_auth(&handle)?;
    let storage = Arc::clone(&state.storage);

    let content = spawn_blocking(move || {
        storage
            .read_user_file(&principal, MEMORY_FILE)
            .unwrap_or_default()
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let updated_at = chrono::Utc::now().to_rfc3339();
    Ok(Json(MemoryDocument { content, updated_at }))
}

async fn save_memory(
    State(state): State<AppState>,
    Extension(handle): Extension<SessionHandle>,
    Json(doc): Json<MemoryDocument>,
) -> StatusCode {
    let principal = match require_auth(&handle) {
        Ok(p) => p,
        Err(s) => return s,
    };

    let storage = Arc::clone(&state.storage);
    match spawn_blocking(move || storage.write_user_file(&principal, MEMORY_FILE, &doc.content))
        .await
    {
        Ok(Ok(_)) => StatusCode::OK,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn append_document_memory(
    State(state): State<AppState>,
    Extension(handle): Extension<SessionHandle>,
    Json(append_req): Json<AppendMemoryRequest>,
) -> StatusCode {
    let principal = match require_auth(&handle) {
        Ok(p) => p,
        Err(s) => return s,
    };

    let storage = Arc::clone(&state.storage);
    let principal_clone = principal.clone();
    let doc_id = append_req.doc_id.clone();
    let config_dir = state.config.server.config_dir.clone();
    let summary_prompt_file = state.config.ai.summary_prompt_file.clone();
    let keypoints_prompt_file = state.config.ai.keypoints_prompt_file.clone();

    let setup = spawn_blocking(move || -> anyhow::Result<_> {
        let toml_str = storage
            .read_user_file(&principal_clone, "settings/ai_config.toml")
            .map_err(|_| anyhow::anyhow!("AI endpoint not configured"))?;
        let user_cfg: UserAiConfig = toml::from_str(&toml_str)?;

        let pdf_path = storage.pdf_path(&principal_clone, &doc_id);
        let doc_context = crate::pdf_text::extract_all_text(&pdf_path)
            .unwrap_or_else(|_| String::new());

        let summary_prompt = crate::ai::load_prompt(
            &PathBuf::from(&config_dir).join(&summary_prompt_file),
            &doc_context,
        )
        .unwrap_or_default();

        let keypoints_prompt = crate::ai::load_prompt(
            &PathBuf::from(&config_dir).join(&keypoints_prompt_file),
            &doc_context,
        )
        .unwrap_or_default();

        let current_memory = storage
            .read_user_file(&principal_clone, MEMORY_FILE)
            .unwrap_or_default();

        Ok((user_cfg, summary_prompt, keypoints_prompt, current_memory, doc_id, storage, principal_clone))
    })
    .await;

    let (user_cfg, summary_prompt, keypoints_prompt, current_memory, doc_id, storage2, principal2) =
        match setup {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => {
                tracing::warn!("append_document_memory setup: {}", e);
                return StatusCode::FAILED_DEPENDENCY;
            }
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR,
        };

    let proxy = AiProxy::new_with_client(state.http_client.clone());

    let summary = match proxy
        .complete(
            &user_cfg,
            &summary_prompt,
            vec![Message {
                role: "user".to_string(),
                content: "Please summarize this document concisely.".to_string(),
            }],
        )
        .await
    {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("AI summary failed: {}", e);
            return StatusCode::BAD_GATEWAY;
        }
    };

    let keypoints = match proxy
        .complete(
            &user_cfg,
            &keypoints_prompt,
            vec![Message {
                role: "user".to_string(),
                content: "List the key points of this document.".to_string(),
            }],
        )
        .await
    {
        Ok(k) => k,
        Err(e) => {
            tracing::warn!("AI keypoints failed: {}", e);
            return StatusCode::BAD_GATEWAY;
        }
    };

    let section = format!(
        "\n\n## Document: {}\n_Added: {}_\n\n### Summary\n{}\n\n### Key Points\n{}\n",
        doc_id,
        chrono::Utc::now().to_rfc3339(),
        summary,
        keypoints,
    );

    let new_memory = format!("{}{}", current_memory, section);

    match spawn_blocking(move || {
        storage2.write_user_file(&principal2, MEMORY_FILE, &new_memory)
    })
    .await
    {
        Ok(Ok(_)) => StatusCode::OK,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
