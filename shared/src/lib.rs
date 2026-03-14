use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct ChatMessage {
    message_id: String,
    user_id: String,
    room_id: String,
    timestamp: String,
    text: String,
}

impl ChatMessage {
    pub fn new(
        message_id: String,
        user_id: String,
        room_id: String,
        timestamp: String,
        text: String,
    ) -> Self {
        Self {
            message_id,
            user_id,
            room_id,
            timestamp,
            text,
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum ClientEvent {
    SendMessage { text: String },
    JoinChannel { room_id: String },
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum ServerEvent {
    NewMessage { message: ChatMessage },
    UserJoined { user_id: String, room_id: String },
    UserLeft { user_id: String, room_id: String },
}
