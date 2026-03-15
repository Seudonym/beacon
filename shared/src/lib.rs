use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChatMessage {
    pub message_id: String,
    pub user_id: String,
    pub room_id: String,
    pub timestamp: String,
    pub text: String,
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

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum ClientEvent {
    SendMessage { text: String },
    JoinChannel { room_id: String },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum ServerEvent {
    NewMessage { message: ChatMessage },
    UserJoined { user_id: String, room_id: String },
    UserLeft { user_id: String, room_id: String },
}
