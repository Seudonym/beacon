use std::{collections::HashMap, str::FromStr, sync::Arc};

use anyhow::Context;
use axum::{
    Form, Router,
    extract::{
        Path, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::StatusCode,
    response::{IntoResponse, Redirect},
    routing::{get, post},
};
use axum_login::{AuthManagerLayerBuilder, AuthSession, AuthUser, AuthnBackend};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use shared::{ChatMessage, ClientEvent, ServerEvent};
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};
use tokio::sync::{RwLock, broadcast};
use tower_sessions::{
    Expiry, SessionManagerLayer,
    cookie::{Key, time::Duration},
};
use tower_sessions_sqlx_store::SqliteStore;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct AppState {
    pub rooms: Arc<RwLock<HashMap<String, broadcast::Sender<String>>>>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct User {
    id: String,
    username: String,
    password_hash: String,
}

impl AuthUser for User {
    type Id = String;
    fn id(&self) -> Self::Id {
        self.id.clone()
    }
    fn session_auth_hash(&self) -> &[u8] {
        self.password_hash.as_bytes()
    }
}

#[derive(Clone, Deserialize)]
struct Credentials {
    username: String,
    password: String,
}

#[derive(Clone)]
struct Backend {
    db: SqlitePool,
}

impl AuthnBackend for Backend {
    type Credentials = Credentials;
    type Error = sqlx::Error;
    type User = User;
    async fn authenticate(
        &self,
        creds: Self::Credentials,
    ) -> Result<Option<Self::User>, Self::Error> {
        let user = sqlx::query_as::<_, User>(
            "select id, username, password_hash from users where username = ?",
        )
        .bind(&creds.username)
        .fetch_optional(&self.db)
        .await?;

        let Some(user) = user else {
            return Ok(None);
        };

        let valid = password_auth::verify_password(&creds.password, &user.password_hash).is_ok();

        Ok(valid.then_some(user))
    }

    async fn get_user(
        &self,
        user_id: &axum_login::UserId<Self>,
    ) -> Result<Option<Self::User>, Self::Error> {
        sqlx::query_as::<_, User>("select id, username, password_hash from users where id = ?")
            .bind(user_id)
            .fetch_optional(&self.db)
            .await
    }
}

type AppAuthSession = AuthSession<Backend>;

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

async fn login(mut auth: AppAuthSession, Form(creds): Form<Credentials>) -> impl IntoResponse {
    let user = match auth.authenticate(creds).await {
        Ok(Some(user)) => user,
        Ok(None) => return StatusCode::UNAUTHORIZED.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    if auth.login(&user).await.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    Redirect::to("/me").into_response()
}

async fn logout(mut auth: AppAuthSession) -> impl IntoResponse {
    if auth.logout().await.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    StatusCode::NO_CONTENT.into_response()
}

async fn me(auth: AppAuthSession) -> impl IntoResponse {
    match auth.user {
        Some(user) => format!("Hello {}", user.username).into_response(),
        None => return StatusCode::UNAUTHORIZED.into_response(),
    }
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    Path(room_id): Path<String>,
    State(state): State<AppState>,
    auth: AppAuthSession,
) -> impl IntoResponse {
    // reject unauthenticated mfs
    let user = match auth.user {
        Some(user) => user,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    ws.on_upgrade(move |socket| handle_socket(socket, room_id, state, user))
}

async fn handle_socket(socket: WebSocket, room_id: String, state: AppState, user: User) {
    // check if just read is enough, else try write to insert channel
    let tx = {
        let rooms = state.rooms.read().await;
        rooms.get(&room_id).cloned()
    };

    // if channel doesnt exist, insert it and acquire channel
    let tx = match tx {
        Some(tx) => tx,
        None => {
            let mut rooms = state.rooms.write().await;
            rooms
                .entry(room_id.clone())
                .or_insert_with(|| {
                    let (tx, _) = broadcast::channel(64);
                    tx
                })
                .clone()
        }
    };

    let mut rx = tx.subscribe();

    // split the socket into sender and reciever
    let (mut ws_sender, mut ws_reciever) = socket.split();

    // send join notification
    let user_id = user.id;

    let join_msg = match serde_json::to_string(&ServerEvent::UserJoined {
        user_id: user_id.clone(),
        room_id: room_id.clone(),
    }) {
        Ok(msg) => msg,
        Err(err) => {
            error!(%err, %user_id, %room_id, "failed to serialize join event");
            return;
        }
    };

    if let Err(err) = tx.send(join_msg) {
        warn!(%err, %user_id, %room_id, "failed to broadcast join event");
    }

    info!(%user_id, %room_id, "user joined room");

    // configure the ws_sender to send messages from rx
    let mut send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            let send = ws_sender.send(Message::Text(msg.into())).await;
            if send.is_err() {
                break;
            }
        }
    });

    // configure ws_reciever to recieve messages and send to tx
    let recv_tx = tx.clone();
    let recv_user_id = user_id.clone();
    let recv_room_id = room_id.clone();
    let mut recv_task = tokio::spawn(async move {
        // keep fetching from client
        while let Some(Ok(msg)) = ws_reciever.next().await {
            match msg {
                Message::Text(text) => {
                    let client_event = match serde_json::from_str::<ClientEvent>(&text) {
                        Ok(event) => event,
                        Err(err) => {
                            warn!(
                                %err,
                                %recv_user_id,
                                %recv_room_id,
                                raw = %text,
                                "failed to parse client event"
                            );
                            continue;
                        }
                    };

                    let message_id = Uuid::new_v4().to_string();
                    match client_event {
                        ClientEvent::SendMessage { text } => {
                            let broadcast_msg =
                                match serde_json::to_string(&ServerEvent::NewMessage {
                                    message: ChatMessage::new(
                                        message_id,
                                        recv_user_id.clone(),
                                        recv_room_id.clone(),
                                        chrono::Utc::now().to_rfc3339(),
                                        text,
                                    ),
                                }) {
                                    Ok(msg) => msg,
                                    Err(err) => {
                                        error!(
                                            %err,
                                            %recv_user_id,
                                            %recv_room_id,
                                            "failed to deserialize send message event"
                                        );
                                        continue;
                                    }
                                };
                            let _ = recv_tx.send(broadcast_msg);
                        }
                        _ => {}
                    }
                }
                Message::Close(_) => {
                    break;
                }
                _ => {}
            }
        }
    });

    // cleanup when any task ends
    tokio::select! {
        _ = &mut recv_task => send_task.abort(),
        _ = &mut send_task=> recv_task.abort()
    }

    let leave_msg = match serde_json::to_string(&ServerEvent::UserLeft {
        user_id: user_id.clone(),
        room_id: room_id.clone(),
    }) {
        Ok(msg) => msg,
        Err(err) => {
            error!(%err, %user_id, %room_id, "failed to serialize join event");
            return;
        }
    };

    let _ = tx.send(leave_msg);

    info!(%user_id, %room_id, "user disconnected");
}
