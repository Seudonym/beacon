use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum ClientEvent {
    SendMessage { text: String },
    JoinChannel { room_id: String },
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum ServerEvent {
    NewMessage { message: String },
    UserJoined { user_id: String, room_id: String },
    Error { message: String },
}
