use axum::{
    extract::State,
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

const GRAPH_FILE: &str = "categories/graph.json";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/graph", get(get_graph).put(save_graph))
}

#[derive(Deserialize, Serialize, Default)]
pub struct CategoryGraph {
    pub nodes: Vec<CategoryNode>,
    pub edges: Vec<CategoryEdge>,
}

#[derive(Deserialize, Serialize)]
pub struct CategoryNode {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub doc_id: Option<String>,
}

#[derive(Deserialize, Serialize)]
pub struct CategoryEdge {
    pub source: String,
    pub target: String,
    pub relation: String,
}

fn require_auth(handle: &SessionHandle) -> Result<Principal, StatusCode> {
    if !handle.data.is_authenticated() {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(Principal::from_session(&handle.data))
}

async fn get_graph(
    State(state): State<AppState>,
    Extension(handle): Extension<SessionHandle>,
) -> Result<Json<CategoryGraph>, StatusCode> {
    let principal = require_auth(&handle)?;
    let storage = Arc::clone(&state.storage);

    let graph = spawn_blocking(move || {
        storage
            .read_user_file(&principal, GRAPH_FILE)
            .ok()
            .and_then(|s| serde_json::from_str::<CategoryGraph>(&s).ok())
            .unwrap_or_default()
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(graph))
}

async fn save_graph(
    State(state): State<AppState>,
    Extension(handle): Extension<SessionHandle>,
    Json(graph): Json<CategoryGraph>,
) -> StatusCode {
    let principal = match require_auth(&handle) {
        Ok(p) => p,
        Err(s) => return s,
    };

    let content = match serde_json::to_string_pretty(&graph) {
        Ok(s) => s,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR,
    };

    let storage = Arc::clone(&state.storage);
    match spawn_blocking(move || storage.write_user_file(&principal, GRAPH_FILE, &content)).await {
        Ok(Ok(_)) => StatusCode::OK,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
