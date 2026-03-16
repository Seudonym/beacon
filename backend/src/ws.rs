use axum::{
    extract::{
        Path, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::StatusCode,
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use shared::{ChatMessage, ClientEvent, ServerEvent};
use tokio::sync::broadcast;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::{
    auth::{AppAuthSession, User},
    state::AppState,
};

pub async fn ws_handler(
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
