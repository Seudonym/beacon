use std::{collections::HashMap, sync::Arc};

use axum::{
    Router,
    extract::{
        Path, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::IntoResponse,
    routing::get,
};
use futures_util::{SinkExt, StreamExt};
use shared::{ClientEvent, ServerEvent};
use tokio::sync::{RwLock, broadcast};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct AppState {
    pub rooms: Arc<RwLock<HashMap<String, broadcast::Sender<String>>>>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

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

    // if channel doesnt exist, insert it and acquire channel
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

    let mut rx = tx.subscribe();

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
        "Sent join notification to user_id: {} in room: {}",
        user_id, room
    );

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
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_reciever.next().await {
            match msg {
                Message::Text(text) => {
                    let client_event = serde_json::from_str::<ClientEvent>(&text).unwrap();
                    match client_event {
                        ClientEvent::SendMessage { text } => {
                            let broadcast_msg =
                                serde_json::to_string(&ServerEvent::NewMessage { message: text })
                                    .unwrap();
                            let _ = tx.send(broadcast_msg);
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

    info!("user_id: {} disconnected from room: {}", user_id, room);
}
