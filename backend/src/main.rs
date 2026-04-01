mod ai;
mod auth;
mod config;
mod gdrive;
mod pdf_text;
mod routes;
mod session;
mod storage;

use axum::{extract::DefaultBodyLimit, middleware, Router};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use session::SessionStore;
use storage::local::LocalStorage;

#[derive(Clone)]
pub struct AppState {
    pub config: config::AppConfig,
    pub storage: Arc<LocalStorage>,
    pub session_store: Arc<SessionStore>,
    pub http_client: reqwest::Client,
}

impl AppState {
    pub async fn new(config: config::AppConfig) -> anyhow::Result<Self> {
        let storage = Arc::new(LocalStorage::new(&config.server.data_dir)?);

        // Derive HMAC secret from env or fall back to a static dev secret
        let secret = std::env::var("SESSION_SECRET")
            .unwrap_or_else(|_| "nimbus-dev-secret-change-in-prod".to_string());

        let session_store = Arc::new(SessionStore::new(
            std::path::Path::new(&config.server.data_dir),
            secret.as_bytes(),
            config.session.anonymous_ttl,
        )?);

        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()?;

        Ok(Self {
            config,
            storage,
            session_store,
            http_client,
        })
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cfg = config::AppConfig::load()?;
    tracing::info!(
        "Starting NimbusPDF backend on {}:{}",
        cfg.server.host,
        cfg.server.port
    );

    let state = AppState::new(cfg).await?;
    let addr = format!("{}:{}", state.config.server.host, state.config.server.port);

    // Session middleware needs state; consume it once via .with_state() at the
    // top level so both the routes and the middleware share the same instance.
    let app = Router::new()
        .nest("/api", routes::router())
        .layer(middleware::from_fn_with_state(
            state.clone(),
            session::session_middleware,
        ))
        .layer(DefaultBodyLimit::max(state.config.server.max_upload_bytes))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Listening on {addr}");
    axum::serve(listener, app).await?;

    Ok(())
}
