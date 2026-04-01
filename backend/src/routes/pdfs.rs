use axum::{
    body::Body,
    extract::{Multipart, Path, State},
    http::{header, HeaderMap, StatusCode},
    response::Response,
    routing::get,
    Extension,
    Json, Router,
};
use std::sync::Arc;
use tokio::task::spawn_blocking;

use crate::session::SessionHandle;
use crate::storage::local::DocMeta;
use crate::storage::Principal;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_pdfs).post(upload_pdf))
        .route("/:doc_id", get(get_pdf_meta).delete(delete_pdf))
        .route("/:doc_id/file", get(serve_pdf))
}

async fn list_pdfs(
    State(state): State<AppState>,
    Extension(handle): Extension<SessionHandle>,
) -> Result<Json<Vec<DocMeta>>, StatusCode> {
    let principal = Principal::from_session(&handle.data);
    let storage = Arc::clone(&state.storage);
    let docs = spawn_blocking(move || storage.list_documents(&principal))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map_err(|e| {
            tracing::error!("list_documents: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(docs))
}

async fn upload_pdf(
    State(state): State<AppState>,
    Extension(handle): Extension<SessionHandle>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<DocMeta>), (StatusCode, Json<serde_json::Value>)> {
    let max_bytes = state.config.server.max_upload_bytes;
    let max_mb = max_bytes / (1024 * 1024);

    // Reject early if Content-Length already exceeds the limit
    if let Some(content_length) = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok())
    {
        if content_length > max_bytes {
            return Err((
                StatusCode::PAYLOAD_TOO_LARGE,
                Json(serde_json::json!({
                    "error": format!("File too large. Maximum upload size is {} MB.", max_mb)
                })),
            ));
        }
    }

    let principal = Principal::from_session(&handle.data);

    let mut pdf_bytes: Option<Vec<u8>> = None;
    let mut filename = "upload.pdf".to_string();

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        // multer surfaces body-limit exhaustion as an incomplete field error
        let msg = if e.to_string().contains("incomplete") || e.to_string().contains("limit") {
            format!("File too large. Maximum upload size is {} MB.", max_mb)
        } else {
            "Failed to read upload data.".to_string()
        };
        (StatusCode::PAYLOAD_TOO_LARGE, Json(serde_json::json!({ "error": msg })))
    })? {
        if let Some(fname) = field.file_name() {
            if !fname.is_empty() {
                filename = fname.to_string();
            }
        }
        let bytes = field.bytes().await.map_err(|e| {
            let msg = if e.to_string().contains("incomplete") || e.to_string().contains("limit") {
                format!("File too large. Maximum upload size is {} MB.", max_mb)
            } else {
                "Failed to read upload data.".to_string()
            };
            (StatusCode::PAYLOAD_TOO_LARGE, Json(serde_json::json!({ "error": msg })))
        })?;
        if !bytes.is_empty() && pdf_bytes.is_none() {
            pdf_bytes = Some(bytes.to_vec());
        }
    }

    let bytes = pdf_bytes.ok_or_else(|| (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": "No PDF file received." })),
    ))?;
    if bytes.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Uploaded file is empty." })),
        ));
    }

    let storage = Arc::clone(&state.storage);
    let filename_clone = filename.clone();
    let bytes_clone = bytes.clone();
    let principal_clone = principal.clone();

    // Save PDF
    let mut meta = spawn_blocking(move || {
        storage.save_pdf(&principal_clone, &filename_clone, &bytes_clone)
    })
    .await
    .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": "Internal error saving file." }))))?
    .map_err(|e| {
        tracing::error!("save_pdf: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": "Failed to save PDF." })))
    })?;

    // Extract page count
    let pdf_path = state.storage.pdf_path(&principal, &meta.id);
    let page_count = spawn_blocking(move || crate::pdf_text::get_page_count(&pdf_path))
        .await
        .ok()
        .and_then(|r| r.ok());

    if let Some(count) = page_count {
        meta.page_count = Some(count);
        let storage2 = Arc::clone(&state.storage);
        let principal2 = principal.clone();
        let meta_clone = meta.clone();
        let doc_id = meta.id.clone();
        let _ = spawn_blocking(move || storage2.write_doc_meta(&principal2, &doc_id, &meta_clone))
            .await;
    }

    Ok((StatusCode::CREATED, Json(meta)))
}

async fn get_pdf_meta(
    State(state): State<AppState>,
    Extension(handle): Extension<SessionHandle>,
    Path(doc_id): Path<String>,
) -> Result<Json<DocMeta>, StatusCode> {
    let principal = Principal::from_session(&handle.data);
    let storage = Arc::clone(&state.storage);
    let meta = spawn_blocking(move || storage.read_doc_meta(&principal, &doc_id))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok(Json(meta))
}

async fn delete_pdf(
    State(state): State<AppState>,
    Extension(handle): Extension<SessionHandle>,
    Path(doc_id): Path<String>,
) -> StatusCode {
    let principal = Principal::from_session(&handle.data);
    let storage = Arc::clone(&state.storage);
    match spawn_blocking(move || storage.delete_document(&principal, &doc_id)).await {
        Ok(Ok(_)) => StatusCode::NO_CONTENT,
        Ok(Err(e)) => {
            tracing::error!("delete_document: {}", e);
            StatusCode::NOT_FOUND
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn serve_pdf(
    State(state): State<AppState>,
    Extension(handle): Extension<SessionHandle>,
    Path(doc_id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let principal = Principal::from_session(&handle.data);
    let pdf_path = state.storage.pdf_path(&principal, &doc_id);

    if !pdf_path.exists() {
        return Err(StatusCode::NOT_FOUND);
    }

    // Check for Range header
    let range_header = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let path_clone = pdf_path.clone();
    let file_bytes = spawn_blocking(move || std::fs::read(&path_clone))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let total_len = file_bytes.len();

    let response = if let Some(range) = range_header {
        if let Some(range_val) = range.strip_prefix("bytes=") {
            let parts: Vec<&str> = range_val.splitn(2, '-').collect();
            let start: usize = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
            let end: usize = parts
                .get(1)
                .and_then(|s| if s.is_empty() { None } else { s.parse().ok() })
                .unwrap_or(total_len.saturating_sub(1));

            let end = end.min(total_len.saturating_sub(1));
            if start > end || start >= total_len {
                return Err(StatusCode::RANGE_NOT_SATISFIABLE);
            }

            let chunk = file_bytes[start..=end].to_vec();
            let content_range = format!("bytes {}-{}/{}", start, end, total_len);

            Response::builder()
                .status(StatusCode::PARTIAL_CONTENT)
                .header(header::CONTENT_TYPE, "application/pdf")
                .header(header::CONTENT_RANGE, content_range)
                .header(header::ACCEPT_RANGES, "bytes")
                .header(header::CONTENT_LENGTH, chunk.len())
                .body(Body::from(chunk))
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        } else {
            build_full_pdf_response(file_bytes, total_len)?
        }
    } else {
        build_full_pdf_response(file_bytes, total_len)?
    };

    Ok(response)
}

fn build_full_pdf_response(bytes: Vec<u8>, len: usize) -> Result<Response, StatusCode> {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/pdf")
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, len)
        .body(Body::from(bytes))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}
