use axum::{
    extract::{Path, State},
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

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/chat", post(chat))
        .route("/summary", post(quick_summary))
        .route("/keypoints", post(quick_keypoints))
        .route("/history/:doc_id", get(get_history))
        .route("/config", get(get_ai_config).post(save_ai_config))
}

#[derive(Deserialize)]
pub struct ChatRequest {
    pub doc_id: String,
    pub message: String,
}

#[derive(Serialize)]
pub struct ChatResponse {
    pub reply: String,
}

#[derive(Deserialize)]
pub struct DocRequest {
    pub doc_id: String,
}

#[derive(Deserialize, Serialize)]
pub struct AiConfigDto {
    pub endpoint_url: String,
    pub model: String,
    #[serde(skip_serializing)]
    pub api_key: Option<String>,
}

#[derive(Deserialize)]
pub struct SaveAiConfigBody {
    pub endpoint_url: String,
    pub model: String,
    pub api_key: Option<String>,
}

/// Load user's ai_config.toml. Returns error if missing.
fn load_user_ai_config(
    storage: &crate::storage::local::LocalStorage,
    principal: &Principal,
) -> anyhow::Result<UserAiConfig> {
    let toml_str = storage
        .read_user_file(principal, "settings/ai_config.toml")
        .map_err(|_| anyhow::anyhow!("AI endpoint not configured"))?;
    toml::from_str::<UserAiConfig>(&toml_str).map_err(|e| anyhow::anyhow!("Invalid ai_config.toml: {}", e))
}

/// Append a message exchange to chat_history.json.
fn append_chat_history(
    storage: &crate::storage::local::LocalStorage,
    principal: &Principal,
    doc_id: &str,
    user_message: &str,
    assistant_reply: &str,
) -> anyhow::Result<()> {
    let existing = storage
        .read_doc_file(principal, doc_id, "chat_history.json")
        .unwrap_or_else(|_| "[]".to_string());
    let mut history: Vec<serde_json::Value> =
        serde_json::from_str(&existing).unwrap_or_default();
    history.push(serde_json::json!({
        "role": "user",
        "content": user_message,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    }));
    history.push(serde_json::json!({
        "role": "assistant",
        "content": assistant_reply,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    }));
    storage.write_doc_file(
        principal,
        doc_id,
        "chat_history.json",
        &serde_json::to_string_pretty(&history)?,
    )
}

async fn chat(
    State(state): State<AppState>,
    Extension(handle): Extension<SessionHandle>,
    Json(chat_req): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, (StatusCode, Json<serde_json::Value>)> {
    let principal = Principal::from_session(&handle.data);

    let storage = Arc::clone(&state.storage);
    let principal_clone = principal.clone();
    let doc_id = chat_req.doc_id.clone();
    let config_dir = state.config.server.config_dir.clone();
    let system_prompt_file = state.config.ai.system_prompt_file.clone();

    let (user_cfg, system_prompt, existing_history) =
        spawn_blocking(move || -> anyhow::Result<_> {
            let user_cfg = load_user_ai_config(&storage, &principal_clone)?;

            let pdf_path = storage.pdf_path(&principal_clone, &doc_id);
            let doc_context = crate::pdf_text::extract_all_text(&pdf_path)
                .unwrap_or_else(|_| String::new());

            let prompt_path = PathBuf::from(&config_dir).join(&system_prompt_file);
            let system_prompt =
                crate::ai::load_prompt(&prompt_path, &doc_context).unwrap_or_default();

            let history_str = storage
                .read_doc_file(&principal_clone, &doc_id, "chat_history.json")
                .unwrap_or_else(|_| "[]".to_string());
            let history: Vec<serde_json::Value> =
                serde_json::from_str(&history_str).unwrap_or_default();

            Ok((user_cfg, system_prompt, history))
        })
        .await
        .map_err(|_| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "setup failed" })),
        ))?
        .map_err(|e| (
            StatusCode::FAILED_DEPENDENCY,
            Json(serde_json::json!({ "error": e.to_string() })),
        ))?;

    // Build message list from history + new message
    let mut messages: Vec<Message> = existing_history
        .iter()
        .filter_map(|v| {
            Some(Message {
                role: v["role"].as_str()?.to_string(),
                content: v["content"].as_str()?.to_string(),
            })
        })
        .collect();
    messages.push(Message {
        role: "user".to_string(),
        content: chat_req.message.clone(),
    });

    let proxy = AiProxy::new_with_client(state.http_client.clone());
    let reply = proxy
        .complete(&user_cfg, &system_prompt, messages)
        .await
        .map_err(|e| (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({ "error": format!("AI request failed: {}", e) })),
        ))?;

    // Persist history
    let storage2 = Arc::clone(&state.storage);
    let principal2 = principal.clone();
    let doc_id2 = chat_req.doc_id.clone();
    let message2 = chat_req.message.clone();
    let reply2 = reply.clone();
    let _ = spawn_blocking(move || {
        append_chat_history(&storage2, &principal2, &doc_id2, &message2, &reply2)
    })
    .await;

    Ok(Json(ChatResponse { reply }))
}

async fn quick_summary(
    State(state): State<AppState>,
    Extension(handle): Extension<SessionHandle>,
    Json(req): Json<DocRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    run_quick_ai_task(state, handle, req.doc_id, "summary").await
}

async fn quick_keypoints(
    State(state): State<AppState>,
    Extension(handle): Extension<SessionHandle>,
    Json(req): Json<DocRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    run_quick_ai_task(state, handle, req.doc_id, "keypoints").await
}

async fn run_quick_ai_task(
    state: AppState,
    handle: SessionHandle,
    doc_id: String,
    task: &str,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let principal = Principal::from_session(&handle.data);

    let prompt_file = match task {
        "summary" => state.config.ai.summary_prompt_file.clone(),
        "keypoints" => state.config.ai.keypoints_prompt_file.clone(),
        _ => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "unknown task" })),
            ))
        }
    };

    let storage = Arc::clone(&state.storage);
    let principal_clone = principal.clone();
    let doc_id_clone = doc_id.clone();
    let config_dir = state.config.server.config_dir.clone();

    let (user_cfg, system_prompt) = spawn_blocking(move || -> anyhow::Result<_> {
        let user_cfg = load_user_ai_config(&storage, &principal_clone)?;

        let pdf_path = storage.pdf_path(&principal_clone, &doc_id_clone);
        let doc_context = crate::pdf_text::extract_all_text(&pdf_path)
            .unwrap_or_else(|_| String::new());

        let prompt_path = PathBuf::from(&config_dir).join(&prompt_file);
        let system_prompt =
            crate::ai::load_prompt(&prompt_path, &doc_context).unwrap_or_default();

        Ok((user_cfg, system_prompt))
    })
    .await
    .map_err(|_| (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": "setup failed" })),
    ))?
    .map_err(|e| (
        StatusCode::FAILED_DEPENDENCY,
        Json(serde_json::json!({ "error": e.to_string() })),
    ))?;

    let proxy = AiProxy::new_with_client(state.http_client.clone());
    let result = proxy
        .complete(
            &user_cfg,
            &system_prompt,
            vec![Message {
                role: "user".to_string(),
                content: format!("Please provide a {} of the document.", task),
            }],
        )
        .await
        .map_err(|e| (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({ "error": format!("AI request failed: {}", e) })),
        ))?;

    Ok(Json(serde_json::json!({ "result": result })))
}

async fn get_history(
    State(state): State<AppState>,
    Extension(handle): Extension<SessionHandle>,
    Path(doc_id): Path<String>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let principal = Principal::from_session(&handle.data);
    let storage = Arc::clone(&state.storage);
    let history_str = spawn_blocking(move || {
        storage
            .read_doc_file(&principal, &doc_id, "chat_history.json")
            .unwrap_or_else(|_| "[]".to_string())
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let history: Vec<serde_json::Value> =
        serde_json::from_str(&history_str).unwrap_or_default();
    Ok(Json(history))
}

async fn get_ai_config(
    State(state): State<AppState>,
    Extension(handle): Extension<SessionHandle>,
) -> Result<Json<AiConfigDto>, (StatusCode, Json<serde_json::Value>)> {
    let principal = Principal::from_session(&handle.data);
    let storage = Arc::clone(&state.storage);

    let cfg = spawn_blocking(move || load_user_ai_config(&storage, &principal))
        .await
        .map_err(|_| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "internal error" })),
        ))?
        .map_err(|_| (
            StatusCode::FAILED_DEPENDENCY,
            Json(serde_json::json!({ "error": "AI endpoint not configured" })),
        ))?;

    Ok(Json(AiConfigDto {
        endpoint_url: cfg.endpoint_url,
        model: cfg.model,
        api_key: None, // never return api_key
    }))
}

async fn save_ai_config(
    State(state): State<AppState>,
    Extension(handle): Extension<SessionHandle>,
    Json(body): Json<SaveAiConfigBody>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let principal = Principal::from_session(&handle.data);
    let storage = Arc::clone(&state.storage);

    spawn_blocking(move || {
        // Read existing to preserve existing api_key if new one not provided
        let existing_key = storage
            .read_user_file(&principal, "settings/ai_config.toml")
            .ok()
            .and_then(|s| toml::from_str::<UserAiConfig>(&s).ok())
            .and_then(|c| c.api_key);

        let api_key = body.api_key.or(existing_key).unwrap_or_default();

        let toml_content = format!(
            "endpoint_url = {:?}\nmodel = {:?}\napi_key = {:?}\n",
            body.endpoint_url, body.model, api_key
        );
        storage.write_user_file(&principal, "settings/ai_config.toml", &toml_content)
    })
    .await
    .map_err(|_| (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": "internal error" })),
    ))?
    .map_err(|e| (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": e.to_string() })),
    ))?;

    Ok(StatusCode::OK)
}
