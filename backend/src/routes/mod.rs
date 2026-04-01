pub mod ai;
pub mod auth;
pub mod categories;
pub mod highlights;
pub mod memory;
pub mod notes;
pub mod pdfs;

use axum::{routing::get, Router};
use crate::AppState;

/// Returns an unfinished Router<AppState>. State is consumed once at the top
/// level in main.rs so that session middleware can also receive it via
/// `from_fn_with_state`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .nest("/pdfs", pdfs::router())
        .nest("/ai", ai::router())
        .nest("/pdfs/:doc_id/highlights", highlights::router())
        .nest("/pdfs/:doc_id/notes", notes::router())
        .nest("/memory", memory::router())
        .nest("/categories", categories::router())
        .nest("/auth", auth::router())
}

async fn health() -> &'static str {
    "ok"
}
