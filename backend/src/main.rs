use anyhow::Context;
use backend::{build_app, connect_db, session_secret_from_env};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // setup env
    let _ = dotenvy::dotenv();

    // setup logging
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    // setup db and app
    let db = connect_db("sqlite://app.db").await?;
    let router = build_app(db, session_secret_from_env()?).await?;

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
