use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SbLoginHello1 {
    CreateRoom(SbLoginHello1CreateRoom),
    JoinRoom(SbLoginHello1JoinRoom),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SbLoginHello1CreateRoom {
    pub room_nickname: String,
    pub user_nickname: String,
    pub max_members: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SbLoginHello1JoinRoom {
    pub room_id: String,
    pub user_nickname: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CbLoginCreateRoom1 {
    pub room_id: String,
}
