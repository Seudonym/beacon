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
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod auth;
mod state;
mod ws;

use auth::{Backend, login, logout, me, register};
use state::AppState;
use ws::ws_handler;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // setup env
    dotenvy::dotenv()?;
    // setup logging
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    // setup db and run migrations
    let options = SqliteConnectOptions::from_str("sqlite://app.db")?.create_if_missing(true);
    let db = SqlitePool::connect_with(options).await?;
    sqlx::migrate!("./migrations").run(&db).await?;

    // setup session store
    let session_store = SqliteStore::new(db.clone());
    session_store.migrate().await?;

    // setup session manager layer with session secret
    let session_secret = std::env::var("SESSION_SECRET").context("SESSION_SECRET must be set")?;
    let secret: [u8; 64] = session_secret
        .as_bytes()
        .try_into()
        .context("SESSION_SECRET must be exactly 64 bytes")?;
    let session_manager_layer = SessionManagerLayer::new(session_store)
        .with_private(Key::from(&secret))
        .with_secure(false)
        .with_expiry(Expiry::OnInactivity(Duration::hours(1)));

    let backend = Backend { db: db.clone() };
    let auth_layer = AuthManagerLayerBuilder::new(backend, session_manager_layer).build();

    // setup app state
    let state = AppState {
        rooms: Arc::new(RwLock::new(HashMap::new())),
    };

    let router = Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/me", get(me))
        .route("/chat/{room}", get(ws_handler))
        .layer(auth_layer)
        .with_state(state);

    let address = "0.0.0.0:3000";
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .with_context(|| format!("failed to bind tcp listener on {address}"))?;

    info!(address, "chat server listening");

    axum::serve(listener, router)
        .await
        .context("axum server exited unexpectedly")?;

    Ok(())
}
