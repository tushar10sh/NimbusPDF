use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::get,
    Extension, Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use rand::RngCore;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::task::spawn_blocking;

use crate::session::{AuthenticatedUser, SessionHandle};
use crate::storage::Principal;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login", get(oidc_login))
        .route("/callback", get(oidc_callback))
        .route("/logout", get(logout))
        .route("/gdrive", get(gdrive_connect))
        .route("/gdrive/callback", get(gdrive_callback))
        .route("/gdrive/disconnect", get(gdrive_disconnect))
        .route("/me", get(me))
}

#[derive(Deserialize)]
pub struct OidcCallbackQuery {
    pub code: String,
    pub state: String,
}

#[derive(Deserialize)]
pub struct GdriveCallbackQuery {
    pub code: String,
}

// ---- OIDC Discovery ----

#[derive(Deserialize, Clone)]
struct OidcDiscovery {
    authorization_endpoint: String,
    token_endpoint: String,
}

async fn fetch_oidc_discovery(
    client: &reqwest::Client,
    issuer: &str,
) -> anyhow::Result<OidcDiscovery> {
    let url = format!(
        "{}/.well-known/openid-configuration",
        issuer.trim_end_matches('/')
    );
    let disc: OidcDiscovery = client.get(&url).send().await?.json().await?;
    Ok(disc)
}

// ---- Helpers ----

fn generate_random_hex(n_bytes: usize) -> String {
    let mut bytes = vec![0u8; n_bytes];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(&bytes)
}

fn generate_random_base64url(n_bytes: usize) -> String {
    let mut bytes = vec![0u8; n_bytes];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(&bytes)
}

fn pkce_challenge(verifier: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let digest = hasher.finalize();
    URL_SAFE_NO_PAD.encode(&digest)
}

fn oidc_issuer() -> Option<String> {
    std::env::var("OIDC_ISSUER_URL").ok()
}

fn google_client_id() -> Option<String> {
    std::env::var("GOOGLE_CLIENT_ID").ok()
}

fn google_client_secret() -> Option<String> {
    std::env::var("GOOGLE_CLIENT_SECRET").ok()
}

fn oidc_client_id() -> Option<String> {
    std::env::var("OIDC_CLIENT_ID").ok()
}

fn oidc_client_secret() -> Option<String> {
    std::env::var("OIDC_CLIENT_SECRET").ok()
}

fn oidc_redirect_uri() -> String {
    std::env::var("OIDC_REDIRECT_URI")
        .unwrap_or_else(|_| "http://localhost:3000/api/auth/callback".to_string())
}

fn gdrive_redirect_uri() -> String {
    std::env::var("GDRIVE_REDIRECT_URI")
        .unwrap_or_else(|_| "http://localhost:3000/api/auth/gdrive/callback".to_string())
}

// ---- JWT helpers ----

/// Decode JWT claims from the payload without verifying signature.
fn decode_jwt_claims(token: &str) -> anyhow::Result<serde_json::Value> {
    let parts: Vec<&str> = token.splitn(3, '.').collect();
    if parts.len() < 2 {
        anyhow::bail!("invalid JWT format");
    }
    let payload = parts[1];
    // Try URL_SAFE_NO_PAD first (standard JWT encoding)
    let decoded = URL_SAFE_NO_PAD
        .decode(payload)
        .or_else(|_| {
            // Add padding and try again
            let padded = match payload.len() % 4 {
                2 => format!("{}==", payload),
                3 => format!("{}=", payload),
                _ => payload.to_string(),
            };
            base64::engine::general_purpose::URL_SAFE.decode(&padded)
        })
        .map_err(|e| anyhow::anyhow!("base64 decode: {}", e))?;
    let claims: serde_json::Value = serde_json::from_slice(&decoded)?;
    Ok(claims)
}

// ---- Route handlers ----

async fn oidc_login(
    State(state): State<AppState>,
    Extension(mut session): Extension<SessionHandle>,
) -> Response {
    let issuer = match oidc_issuer() {
        Some(i) => i,
        None => return StatusCode::NOT_FOUND.into_response(),
    };

    let client_id = match oidc_client_id() {
        Some(id) => id,
        None => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "OIDC_CLIENT_ID not set")
                .into_response()
        }
    };

    // Generate PKCE + CSRF
    let state_val = generate_random_hex(32);
    let nonce = generate_random_hex(32);
    let code_verifier = generate_random_base64url(43);
    let code_challenge = pkce_challenge(&code_verifier);
    let redirect_uri = oidc_redirect_uri();

    // Fetch discovery
    let discovery = match fetch_oidc_discovery(&state.http_client, &issuer).await {
        Ok(d) => d,
        Err(e) => {
            tracing::error!("OIDC discovery failed: {}", e);
            return (StatusCode::BAD_GATEWAY, "OIDC discovery failed").into_response();
        }
    };

    // Store in session
    session.data.oidc_csrf = Some(state_val.clone());
    session.data.oidc_nonce = Some(nonce.clone());
    session.data.oidc_pkce_verifier = Some(code_verifier);

    let store = Arc::clone(&state.session_store);
    let session_data = session.data.clone();
    let _ = spawn_blocking(move || store.save(&session_data)).await;

    // Build redirect URL
    let auth_url = format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}&nonce={}&code_challenge={}&code_challenge_method=S256",
        discovery.authorization_endpoint,
        utf8_percent_encode(&client_id, NON_ALPHANUMERIC),
        utf8_percent_encode(&redirect_uri, NON_ALPHANUMERIC),
        utf8_percent_encode("openid email profile", NON_ALPHANUMERIC),
        state_val,
        nonce,
        code_challenge,
    );

    Redirect::temporary(&auth_url).into_response()
}

async fn oidc_callback(
    State(state): State<AppState>,
    Extension(mut session): Extension<SessionHandle>,
    Query(q): Query<OidcCallbackQuery>,
) -> Response {
    let issuer = match oidc_issuer() {
        Some(i) => i,
        None => return StatusCode::NOT_FOUND.into_response(),
    };

    let client_id = match oidc_client_id() {
        Some(id) => id,
        None => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let client_secret = oidc_client_secret().unwrap_or_default();
    let redirect_uri = oidc_redirect_uri();

    // Validate state
    let expected_state = match &session.data.oidc_csrf {
        Some(s) => s.clone(),
        None => return (StatusCode::BAD_REQUEST, "missing CSRF state").into_response(),
    };

    if q.state != expected_state {
        return (StatusCode::BAD_REQUEST, "invalid CSRF state").into_response();
    }

    let code_verifier = match session.data.oidc_pkce_verifier.clone() {
        Some(v) => v,
        None => return (StatusCode::BAD_REQUEST, "missing PKCE verifier").into_response(),
    };

    // Fetch discovery for token endpoint
    let discovery = match fetch_oidc_discovery(&state.http_client, &issuer).await {
        Ok(d) => d,
        Err(e) => {
            tracing::error!("OIDC discovery failed: {}", e);
            return (StatusCode::BAD_GATEWAY, "OIDC discovery failed").into_response();
        }
    };

    // Exchange code for tokens
    let params = [
        ("grant_type", "authorization_code"),
        ("code", q.code.as_str()),
        ("redirect_uri", redirect_uri.as_str()),
        ("client_id", client_id.as_str()),
        ("client_secret", client_secret.as_str()),
        ("code_verifier", code_verifier.as_str()),
    ];

    let token_resp: serde_json::Value = match state
        .http_client
        .post(&discovery.token_endpoint)
        .form(&params)
        .send()
        .await
    {
        Ok(r) => match r.json().await {
            Ok(v) => v,
            Err(e) => {
                tracing::error!("token response parse: {}", e);
                return StatusCode::BAD_GATEWAY.into_response();
            }
        },
        Err(e) => {
            tracing::error!("token exchange: {}", e);
            return StatusCode::BAD_GATEWAY.into_response();
        }
    };

    let id_token = match token_resp["id_token"].as_str() {
        Some(t) => t.to_string(),
        None => {
            tracing::error!("no id_token in response: {:?}", token_resp);
            return StatusCode::BAD_GATEWAY.into_response();
        }
    };

    let claims = match decode_jwt_claims(&id_token) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("JWT decode: {}", e);
            return StatusCode::BAD_GATEWAY.into_response();
        }
    };

    let subject = match claims["sub"].as_str() {
        Some(s) => s.to_string(),
        None => return (StatusCode::BAD_GATEWAY, "no sub claim").into_response(),
    };

    let email = claims["email"].as_str().map(|s| s.to_string());
    let name = claims["name"].as_str().map(|s| s.to_string());

    // Update session
    session.data.user = Some(AuthenticatedUser {
        subject,
        email,
        name,
        gdrive_connected: false,
    });
    session.data.oidc_csrf = None;
    session.data.oidc_nonce = None;
    session.data.oidc_pkce_verifier = None;

    let store = Arc::clone(&state.session_store);
    let session_data = session.data.clone();
    let _ = spawn_blocking(move || store.save(&session_data)).await;

    Redirect::temporary("/").into_response()
}

async fn logout(
    State(state): State<AppState>,
    Extension(session): Extension<SessionHandle>,
) -> Response {
    let store = Arc::clone(&state.session_store);
    let sid = session.data.session_id.clone();
    let _ = spawn_blocking(move || store.delete(&sid)).await;

    let cookie_name = &state.config.session.cookie_name;
    let clear_cookie = format!(
        "{}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0",
        cookie_name
    );

    let mut resp = Redirect::temporary("/").into_response();
    if let Ok(val) = axum::http::HeaderValue::from_str(&clear_cookie) {
        resp.headers_mut()
            .insert(axum::http::header::SET_COOKIE, val);
    }
    resp
}

async fn gdrive_connect(
    State(_state): State<AppState>,
    Extension(handle): Extension<SessionHandle>,
) -> Response {
    if !handle.data.is_authenticated() {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let client_id = match google_client_id() {
        Some(id) => id,
        None => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "GOOGLE_CLIENT_ID not set")
                .into_response()
        }
    };

    let redirect_uri = gdrive_redirect_uri();
    let auth_url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent",
        utf8_percent_encode(&client_id, NON_ALPHANUMERIC),
        utf8_percent_encode(&redirect_uri, NON_ALPHANUMERIC),
        utf8_percent_encode("https://www.googleapis.com/auth/drive.file", NON_ALPHANUMERIC),
    );

    Redirect::temporary(&auth_url).into_response()
}

async fn gdrive_callback(
    State(state): State<AppState>,
    Extension(mut session): Extension<SessionHandle>,
    Query(q): Query<GdriveCallbackQuery>,
) -> Response {
    if !session.data.is_authenticated() {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let client_id = match google_client_id() {
        Some(id) => id,
        None => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let client_secret = match google_client_secret() {
        Some(s) => s,
        None => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let redirect_uri = gdrive_redirect_uri();

    let params = [
        ("code", q.code.as_str()),
        ("client_id", client_id.as_str()),
        ("client_secret", client_secret.as_str()),
        ("redirect_uri", redirect_uri.as_str()),
        ("grant_type", "authorization_code"),
    ];

    let token_resp: serde_json::Value = match state
        .http_client
        .post("https://oauth2.googleapis.com/token")
        .form(&params)
        .send()
        .await
    {
        Ok(r) => match r.json().await {
            Ok(v) => v,
            Err(e) => {
                tracing::error!("gdrive token parse: {}", e);
                return StatusCode::BAD_GATEWAY.into_response();
            }
        },
        Err(e) => {
            tracing::error!("gdrive token exchange: {}", e);
            return StatusCode::BAD_GATEWAY.into_response();
        }
    };

    // Compute expires_at
    let expires_in = token_resp["expires_in"].as_u64().unwrap_or(3600);
    let mut token_with_expiry = token_resp.clone();
    token_with_expiry["expires_at"] =
        serde_json::json!(chrono::Utc::now().timestamp() + expires_in as i64);

    // Save token to settings/gdrive_token.json
    let principal = Principal::from_session(&session.data);
    let token_json = match serde_json::to_string_pretty(&token_with_expiry) {
        Ok(s) => s,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let storage = Arc::clone(&state.storage);
    if let Err(e) = spawn_blocking(move || {
        storage.write_user_file(&principal, "settings/gdrive_token.json", &token_json)
    })
    .await
    .unwrap_or_else(|_| Err(anyhow::anyhow!("spawn error")))
    {
        tracing::error!("saving gdrive token: {}", e);
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    // Update session
    if let Some(ref mut user) = session.data.user {
        user.gdrive_connected = true;
    }
    let store = Arc::clone(&state.session_store);
    let session_data = session.data.clone();
    let _ = spawn_blocking(move || store.save(&session_data)).await;

    Redirect::temporary("/settings").into_response()
}

async fn gdrive_disconnect(
    State(state): State<AppState>,
    Extension(mut session): Extension<SessionHandle>,
) -> StatusCode {
    if !session.data.is_authenticated() {
        return StatusCode::UNAUTHORIZED;
    }

    let principal = Principal::from_session(&session.data);
    let storage = Arc::clone(&state.storage);

    // Delete token file
    let _ = spawn_blocking(move || {
        let path = storage
            .root_for(&principal)
            .join("settings")
            .join("gdrive_token.json");
        if path.exists() {
            std::fs::remove_file(&path).ok();
        }
    })
    .await;

    // Update session
    if let Some(ref mut user) = session.data.user {
        user.gdrive_connected = false;
    }
    let store = Arc::clone(&state.session_store);
    let session_data = session.data.clone();
    let _ = spawn_blocking(move || store.save(&session_data)).await;

    StatusCode::OK
}

async fn me(
    Extension(session): Extension<SessionHandle>,
) -> Json<serde_json::Value> {
    let authenticated = session.data.is_authenticated();
    let (email, name, gdrive_connected) = match &session.data.user {
        Some(u) => (u.email.clone(), u.name.clone(), u.gdrive_connected),
        None => (None, None, false),
    };
    Json(serde_json::json!({
        "authenticated": authenticated,
        "email": email,
        "name": name,
        "gdrive_connected": gdrive_connected,
    }))
}
