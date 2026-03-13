use std::{collections::HashMap, sync::Arc};

use axum::{
    Router,
    extract::{Path, State, WebSocketUpgrade, ws::WebSocket},
    response::IntoResponse,
    routing::get,
};
use futures_util::{SinkExt, StreamExt};
use shared::ServerEvent;
use tokio::sync::{RwLock, broadcast};
use tracing::info;
use tracing_subscriber::layer::SubscriberExt;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct AppState {
    pub rooms: Arc<RwLock<HashMap<String, broadcast::Sender<String>>>>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env());

    let state = AppState {
        rooms: Arc::new(RwLock::new(HashMap::new())),
    };

    let router = Router::new()
        .route("/chat/{room}", get(ws_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    tracing::info!("Server listening on ws://0.0.0.0:3000");
    axum::serve(listener, router).await.unwrap();
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    Path(room): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, room, state))
}

async fn handle_socket(socket: WebSocket, room: String, state: AppState) {
    // check if just read is enough, else try write to insert channel
    let tx = {
        let rooms = state.rooms.read().await;
        rooms.get(&room).cloned()
    };

    let tx = match tx {
        Some(tx) => tx,
        None => {
            let mut rooms = state.rooms.write().await;
            rooms
                .entry(room.clone())
                .or_insert_with(|| {
                    let (tx, _) = broadcast::channel(64);
                    tx
                })
                .clone()
        }
    };

    let rx = tx.subscribe();

    // split the socket into sender and reciever
    let (mut ws_sender, mut ws_reciever) = socket.split();

    // send join notification
    let user_id = Uuid::new_v4().to_string();
    let join_msg = serde_json::to_string(&ServerEvent::UserJoined {
        user_id: user_id.clone(),
        room_id: room.clone(),
    })
    .unwrap();
    let _ = ws_sender
        .send(axum::extract::ws::Message::Text(join_msg.into()))
        .await;

    info!(
        "Send join notification to user_id: {} in room: {}",
        user_id, room
    )

    // configure the ws_sender to send messages from rx
    // configure ws_reciever to recieve messages and send to tx

    // cleanup when any task ends
}
