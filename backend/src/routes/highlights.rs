use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Extension, Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::task::spawn_blocking;

use crate::session::SessionHandle;
use crate::storage::Principal;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(get_highlights).put(save_highlights))
}

#[derive(Deserialize, Serialize, Clone)]
pub struct Highlight {
    pub id: String,
    pub page: u32,
    pub text: String,
    pub color: String,
    pub range_start: usize,
    pub range_end: usize,
    pub note: Option<String>,
}

async fn get_highlights(
    State(state): State<AppState>,
    Extension(handle): Extension<SessionHandle>,
    Path(doc_id): Path<String>,
) -> Result<Json<Vec<Highlight>>, StatusCode> {
    let principal = Principal::from_session(&handle.data);
    let storage = Arc::clone(&state.storage);

    let contents = spawn_blocking(move || {
        storage
            .read_doc_file(&principal, &doc_id, "highlights.json")
            .unwrap_or_else(|_| "[]".to_string())
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let highlights: Vec<Highlight> = serde_json::from_str(&contents).unwrap_or_default();
    Ok(Json(highlights))
}

async fn save_highlights(
    State(state): State<AppState>,
    Extension(handle): Extension<SessionHandle>,
    Path(doc_id): Path<String>,
    Json(highlights): Json<Vec<Highlight>>,
) -> StatusCode {
    let principal = Principal::from_session(&handle.data);

    let content = match serde_json::to_string_pretty(&highlights) {
        Ok(s) => s,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR,
    };

    let storage = Arc::clone(&state.storage);
    match spawn_blocking(move || {
        storage.write_doc_file(&principal, &doc_id, "highlights.json", &content)
    })
    .await
    {
        Ok(Ok(_)) => StatusCode::OK,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
