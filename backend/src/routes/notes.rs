use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Extension, Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::task::spawn_blocking;

use crate::session::SessionHandle;
use crate::storage::Principal;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(get_notes).put(save_notes))
        .route("/:page", get(get_page_note).put(save_page_note))
}

#[derive(Deserialize, Serialize, Clone)]
pub struct PageNote {
    pub page: u32,
    pub content: String,
    pub updated_at: String,
}

#[derive(Deserialize, Serialize, Clone)]
struct NoteEntry {
    content: String,
    updated_at: String,
}

fn load_notes_map(
    storage: &crate::storage::local::LocalStorage,
    principal: &Principal,
    doc_id: &str,
) -> HashMap<String, NoteEntry> {
    storage
        .read_doc_file(principal, doc_id, "notes.json")
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

async fn get_notes(
    State(state): State<AppState>,
    Extension(handle): Extension<SessionHandle>,
    Path(doc_id): Path<String>,
) -> Result<Json<Vec<PageNote>>, StatusCode> {
    let principal = Principal::from_session(&handle.data);
    let storage = Arc::clone(&state.storage);

    let notes_map = spawn_blocking(move || load_notes_map(&storage, &principal, &doc_id))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut notes: Vec<PageNote> = notes_map
        .iter()
        .filter_map(|(page_str, entry)| {
            page_str.parse::<u32>().ok().map(|page| PageNote {
                page,
                content: entry.content.clone(),
                updated_at: entry.updated_at.clone(),
            })
        })
        .collect();

    notes.sort_by_key(|n| n.page);
    Ok(Json(notes))
}

async fn save_notes(
    State(state): State<AppState>,
    Extension(handle): Extension<SessionHandle>,
    Path(doc_id): Path<String>,
    Json(notes): Json<Vec<PageNote>>,
) -> StatusCode {
    let principal = Principal::from_session(&handle.data);

    let mut map: HashMap<String, NoteEntry> = HashMap::new();
    for note in notes {
        map.insert(
            note.page.to_string(),
            NoteEntry {
                content: note.content,
                updated_at: note.updated_at,
            },
        );
    }

    let content = match serde_json::to_string_pretty(&map) {
        Ok(s) => s,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR,
    };

    let storage = Arc::clone(&state.storage);
    match spawn_blocking(move || {
        storage.write_doc_file(&principal, &doc_id, "notes.json", &content)
    })
    .await
    {
        Ok(Ok(_)) => StatusCode::OK,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn get_page_note(
    State(state): State<AppState>,
    Extension(handle): Extension<SessionHandle>,
    Path((doc_id, page)): Path<(String, u32)>,
) -> Result<Json<PageNote>, StatusCode> {
    let principal = Principal::from_session(&handle.data);
    let storage = Arc::clone(&state.storage);

    let notes_map = spawn_blocking(move || load_notes_map(&storage, &principal, &doc_id))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let entry = notes_map
        .get(&page.to_string())
        .cloned()
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(PageNote {
        page,
        content: entry.content,
        updated_at: entry.updated_at,
    }))
}

async fn save_page_note(
    State(state): State<AppState>,
    Extension(handle): Extension<SessionHandle>,
    Path((doc_id, page)): Path<(String, u32)>,
    Json(note_input): Json<serde_json::Value>,
) -> StatusCode {
    let principal = Principal::from_session(&handle.data);

    let content = match note_input["content"].as_str() {
        Some(c) => c.to_string(),
        None => return StatusCode::BAD_REQUEST,
    };

    let storage = Arc::clone(&state.storage);
    match spawn_blocking(move || {
        let mut notes_map = load_notes_map(&storage, &principal, &doc_id);
        notes_map.insert(
            page.to_string(),
            NoteEntry {
                content,
                updated_at: chrono::Utc::now().to_rfc3339(),
            },
        );
        let serialized = serde_json::to_string_pretty(&notes_map)?;
        storage.write_doc_file(&principal, &doc_id, "notes.json", &serialized)
    })
    .await
    {
        Ok(Ok(_)) => StatusCode::OK,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
