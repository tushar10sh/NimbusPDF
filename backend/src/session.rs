use anyhow::{Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use ring::hmac;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
#[allow(unused_imports)]
use std::sync::Mutex;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthenticatedUser {
    pub subject: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub gdrive_connected: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionData {
    pub session_id: String,
    pub user: Option<AuthenticatedUser>,
    pub oidc_csrf: Option<String>,
    pub oidc_nonce: Option<String>,
    pub oidc_pkce_verifier: Option<String>,
    pub created_at: i64,
    pub expires_at: i64,
}

impl SessionData {
    pub fn anonymous(session_id: String, ttl_secs: u64) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            session_id,
            user: None,
            oidc_csrf: None,
            oidc_nonce: None,
            oidc_pkce_verifier: None,
            created_at: now,
            expires_at: now + ttl_secs as i64,
        }
    }

    pub fn is_authenticated(&self) -> bool {
        self.user.is_some()
    }

    pub fn is_expired(&self) -> bool {
        chrono::Utc::now().timestamp() > self.expires_at
    }
}

#[derive(Clone)]
pub struct SessionStore {
    sessions_dir: PathBuf,
    key: Arc<hmac::Key>,
    pub anonymous_ttl: u64,
}

impl SessionStore {
    pub fn new(data_dir: &Path, secret: &[u8], anonymous_ttl: u64) -> Result<Self> {
        let sessions_dir = data_dir.join("sessions");
        std::fs::create_dir_all(&sessions_dir)?;
        let key = hmac::Key::new(hmac::HMAC_SHA256, secret);
        Ok(Self {
            sessions_dir,
            key: Arc::new(key),
            anonymous_ttl,
        })
    }

    /// Sign a session_id and return a cookie value: `<id>.<base64url_sig>`
    pub fn sign_session_id(&self, session_id: &str) -> String {
        let sig = hmac::sign(&self.key, session_id.as_bytes());
        let encoded = URL_SAFE_NO_PAD.encode(sig.as_ref());
        format!("{}.{}", session_id, encoded)
    }

    /// Verify a cookie value; returns the session_id if signature is valid.
    pub fn verify_cookie(&self, cookie_value: &str) -> Option<String> {
        let (session_id, sig_b64) = cookie_value.rsplit_once('.')?;
        let sig_bytes = URL_SAFE_NO_PAD.decode(sig_b64).ok()?;
        hmac::verify(&self.key, session_id.as_bytes(), &sig_bytes).ok()?;
        Some(session_id.to_string())
    }

    /// Save session data to disk.
    pub fn save(&self, session: &SessionData) -> Result<()> {
        let path = self.sessions_dir.join(format!("{}.json", session.session_id));
        let contents = serde_json::to_string_pretty(session)?;
        std::fs::write(&path, contents)
            .with_context(|| format!("writing session {}", session.session_id))
    }

    /// Load session data from disk.
    pub fn load(&self, session_id: &str) -> Result<SessionData> {
        let path = self.sessions_dir.join(format!("{}.json", session_id));
        let contents = std::fs::read_to_string(&path)
            .with_context(|| format!("reading session {}", session_id))?;
        let session: SessionData = serde_json::from_str(&contents)?;
        Ok(session)
    }

    /// Delete a session from disk.
    pub fn delete(&self, session_id: &str) -> Result<()> {
        let path = self.sessions_dir.join(format!("{}.json", session_id));
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }

    /// Remove all expired sessions from disk.
    pub fn cleanup_expired(&self) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        for entry in std::fs::read_dir(&self.sessions_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Ok(contents) = std::fs::read_to_string(&path) {
                if let Ok(session) = serde_json::from_str::<SessionData>(&contents) {
                    if session.expires_at < now {
                        let _ = std::fs::remove_file(&path);
                    }
                }
            }
        }
        Ok(())
    }
}

// ---- Axum middleware ----

use axum::{
    extract::Request,
    middleware::Next,
    response::Response,
};

/// Extension set by middleware — wraps the session and a "dirty" flag.
#[derive(Clone)]
pub struct SessionHandle {
    pub data: SessionData,
    pub is_new: bool,
}

pub async fn session_middleware(
    axum::extract::State(state): axum::extract::State<crate::AppState>,
    mut req: Request,
    next: Next,
) -> Response {
    let store = &state.session_store;
    let cookie_name = &state.config.session.cookie_name;

    // Try to extract the session cookie
    let cookie_header = req
        .headers()
        .get(axum::http::header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let existing_session_id = extract_cookie_value(&cookie_header, cookie_name)
        .and_then(|val| store.verify_cookie(val));

    let (session, is_new) = match existing_session_id {
        Some(ref sid) => {
            match store.load(sid) {
                Ok(s) if !s.is_expired() => (s, false),
                _ => {
                    // Expired or unreadable — create new
                    let new_sid = uuid::Uuid::new_v4().to_string();
                    let s = SessionData::anonymous(new_sid, store.anonymous_ttl);
                    let _ = store.save(&s);
                    (s, true)
                }
            }
        }
        None => {
            let new_sid = uuid::Uuid::new_v4().to_string();
            let s = SessionData::anonymous(new_sid, store.anonymous_ttl);
            let _ = store.save(&s);
            (s, true)
        }
    };

    let handle = SessionHandle {
        data: session.clone(),
        is_new,
    };
    req.extensions_mut().insert(handle);

    let mut response = next.run(req).await;

    // After processing: check if cookie needs updating
    // We always set cookie for new sessions or if the extension was mutated
    // Retrieve updated handle from response extensions if present
    let should_set_cookie = is_new;
    if should_set_cookie {
        let cookie_value = store.sign_session_id(&session.session_id);
        let cookie_str = format!(
            "{}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}",
            cookie_name,
            cookie_value,
            store.anonymous_ttl
        );
        if let Ok(val) = axum::http::HeaderValue::from_str(&cookie_str) {
            response
                .headers_mut()
                .insert(axum::http::header::SET_COOKIE, val);
        }
    }

    response
}

fn extract_cookie_value<'a>(header: &'a str, name: &str) -> Option<&'a str> {
    for part in header.split(';') {
        let part = part.trim();
        if let Some(rest) = part.strip_prefix(name) {
            if let Some(val) = rest.strip_prefix('=') {
                return Some(val.trim());
            }
        }
    }
    None
}

/// Helper to update and persist session data during a request.
/// Call this from route handlers after mutating session.
pub fn set_session_cookie(response: &mut Response, session: &SessionData, store: &SessionStore, cookie_name: &str) {
    let cookie_value = store.sign_session_id(&session.session_id);
    let cookie_str = format!(
        "{}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}",
        cookie_name,
        cookie_value,
        store.anonymous_ttl
    );
    if let Ok(val) = axum::http::HeaderValue::from_str(&cookie_str) {
        response
            .headers_mut()
            .insert(axum::http::header::SET_COOKIE, val);
    }
}

/// Extension helper: get session from request extensions.
pub fn get_session(extensions: &axum::http::Extensions) -> Option<&SessionData> {
    extensions.get::<SessionHandle>().map(|h| &h.data)
}
