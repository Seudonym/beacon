use anyhow::Context;
use axum::{
    Router,
    routing::{get, post},
};
use axum_login::AuthManagerLayerBuilder;
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};
use std::{collections::HashMap, str::FromStr, sync::Arc};
use tokio::sync::RwLock;
use tower_sessions::{
    Expiry, SessionManagerLayer,
    cookie::{Key, time::Duration},
};
use tower_sessions_sqlx_store::SqliteStore;

pub mod auth;
pub mod state;
pub mod ws;

use auth::{Backend, login, logout, me, register};
use state::AppState;
use ws::ws_handler;

pub async fn connect_db(database_url: &str) -> anyhow::Result<SqlitePool> {
    let options = SqliteConnectOptions::from_str(database_url)?.create_if_missing(true);
    let db = SqlitePool::connect_with(options).await?;
    sqlx::migrate!("./migrations").run(&db).await?;
    Ok(db)
}

pub async fn build_app(db: SqlitePool, session_secret: [u8; 64]) -> anyhow::Result<Router> {
    let session_store = SqliteStore::new(db.clone());
    session_store.migrate().await?;

    let session_manager_layer = SessionManagerLayer::new(session_store)
        .with_private(Key::from(&session_secret))
        .with_secure(false)
        .with_expiry(Expiry::OnInactivity(Duration::hours(1)));

    let backend = Backend { db };
    let auth_layer = AuthManagerLayerBuilder::new(backend, session_manager_layer).build();

    let state = AppState {
        rooms: Arc::new(RwLock::new(HashMap::new())),
    };

    Ok(Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/me", get(me))
        .route("/chat/{room}", get(ws_handler))
        .layer(auth_layer)
        .with_state(state))
}

pub fn session_secret_from_env() -> anyhow::Result<[u8; 64]> {
    let session_secret = std::env::var("SESSION_SECRET").context("SESSION_SECRET must be set")?;
    session_secret
        .as_bytes()
        .try_into()
        .context("SESSION_SECRET must be exactly 64 bytes")
}
