use anyhow::Context;
use axum::{
    Router,
    http::{HeaderValue, Method},
    routing::{get, post},
};
use axum_login::AuthManagerLayerBuilder;
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};
use std::{collections::HashMap, str::FromStr, sync::Arc};
use tokio::sync::RwLock;
use tower_http::cors::{AllowOrigin, CorsLayer};
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
        .with_secure(session_cookie_secure())
        .with_expiry(Expiry::OnInactivity(Duration::hours(1)));

    let backend = Backend { db };
    let auth_layer = AuthManagerLayerBuilder::new(backend, session_manager_layer).build();

    let state = AppState {
        rooms: Arc::new(RwLock::new(HashMap::new())),
    };

    let cors = CorsLayer::new()
        .allow_credentials(true)
        .allow_methods([Method::GET, Method::POST])
        .allow_origin(AllowOrigin::list([
            HeaderValue::from_static("http://localhost:8080"),
            HeaderValue::from_static("http://127.0.0.1:8080"),
            HeaderValue::from_static("http://localhost:3000"),
            HeaderValue::from_static("http://127.0.0.1:3000"),
        ]));

    Ok(Router::new()
        .route("/api/register", post(register))
        .route("/api/login", post(login))
        .route("/api/logout", post(logout))
        .route("/api/me", get(me))
        .route("/ws/{room}", get(ws_handler))
        .layer(cors)
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

fn session_cookie_secure() -> bool {
    match std::env::var("COOKIE_SECURE") {
        Ok(value) => !matches!(value.trim().to_ascii_lowercase().as_str(), "0" | "false" | "no"),
        Err(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::{build_app, connect_db};
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode, header},
    };
    use tokio::task::JoinHandle;
    use tokio_tungstenite::{connect_async, tungstenite::Error as WsError};
    use tower::ServiceExt;

    fn test_secret() -> [u8; 64] {
        *b"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
    }

    async fn test_app() -> axum::Router {
        let db_path = format!("/tmp/beacon-test-{}.db", uuid::Uuid::new_v4());
        let db = connect_db(&format!("sqlite://{db_path}"))
            .await
            .expect("connect test db");
        build_app(db, test_secret()).await.expect("build test app")
    }

    async fn spawn_test_server() -> (String, JoinHandle<()>) {
        let app = test_app().await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test listener");
        let addr = listener.local_addr().expect("listener addr");
        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve test app");
        });
        (format!("ws://{addr}"), handle)
    }

    fn cookie_header(response: &axum::response::Response) -> String {
        response
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .find_map(|value| value.to_str().ok().map(str::to_owned))
            .expect("set-cookie header")
    }

    #[tokio::test]
    async fn register_logs_user_in_and_me_returns_user() {
        let app = test_app().await;

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/register")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from("username=alice&password=secret123"))
                    .expect("register request"),
            )
            .await
            .expect("register response");

        assert_eq!(response.status(), StatusCode::OK);

        let cookie = cookie_header(&response);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/me")
                    .header(header::COOKIE, cookie)
                    .body(Body::empty())
                    .expect("me request"),
            )
            .await
            .expect("me response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read me body");
        assert_eq!(
            std::str::from_utf8(&body).expect("utf8 body"),
            r#"{"username":"alice"}"#
        );
    }

    #[tokio::test]
    async fn duplicate_register_returns_conflict() {
        let app = test_app().await;

        let first = Request::builder()
            .method("POST")
            .uri("/api/register")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from("username=alice&password=secret123"))
            .expect("first register request");

        let response = app.clone().oneshot(first).await.expect("first register");
        assert_eq!(response.status(), StatusCode::OK);

        let second = Request::builder()
            .method("POST")
            .uri("/api/register")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from("username=alice&password=secret123"))
            .expect("second register request");

        let response = app.oneshot(second).await.expect("second register");
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn login_rejects_wrong_password() {
        let app = test_app().await;

        let register = Request::builder()
            .method("POST")
            .uri("/api/register")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from("username=alice&password=secret123"))
            .expect("register request");
        let response = app.clone().oneshot(register).await.expect("register");
        assert_eq!(response.status(), StatusCode::OK);

        let login = Request::builder()
            .method("POST")
            .uri("/api/login")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from("username=alice&password=wrongpass"))
            .expect("login request");
        let response = app.oneshot(login).await.expect("login");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn login_succeeds_and_sets_session_cookie() {
        let app = test_app().await;

        let register = Request::builder()
            .method("POST")
            .uri("/api/register")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from("username=alice&password=secret123"))
            .expect("register request");
        let response = app.clone().oneshot(register).await.expect("register");
        assert_eq!(response.status(), StatusCode::OK);

        let cookie = cookie_header(&response);

        let logout = Request::builder()
            .method("POST")
            .uri("/api/logout")
            .header(header::COOKIE, cookie)
            .body(Body::empty())
            .expect("logout request");
        let response = app.clone().oneshot(logout).await.expect("logout");
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        let login = Request::builder()
            .method("POST")
            .uri("/api/login")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from("username=alice&password=secret123"))
            .expect("login request");
        let response = app.clone().oneshot(login).await.expect("login");

        assert_eq!(response.status(), StatusCode::OK);

        let cookie = cookie_header(&response);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/me")
                    .header(header::COOKIE, cookie)
                    .body(Body::empty())
                    .expect("me request"),
            )
            .await
            .expect("me response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read me body");
        assert_eq!(
            std::str::from_utf8(&body).expect("utf8 body"),
            r#"{"username":"alice"}"#
        );
    }

    #[tokio::test]
    async fn me_requires_session() {
        let app = test_app().await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/me")
                    .body(Body::empty())
                    .expect("me request"),
            )
            .await
            .expect("me response");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn websocket_upgrade_requires_session() {
        let (base_url, server) = spawn_test_server().await;

        let result = connect_async(format!("{base_url}/ws/test-room")).await;
        server.abort();

        match result {
            Err(WsError::Http(response)) => assert_eq!(response.status(), StatusCode::UNAUTHORIZED),
            other => panic!("expected unauthorized websocket handshake failure, got {other:?}"),
        }
    }
}
