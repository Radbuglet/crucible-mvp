use bytemuck::{Pod, Zeroable};
use wasmlink::{Marshal, PodMarshal, Port, marshal_struct};

pub const GAME_SOCKET_GET_ID: Port<GameSocketHandle, u64> =
    Port::new("crucible", "game_socket_get_id");

pub const GAME_SOCKET_GET_RTT: Port<GameSocketHandle, Option<f64>> =
    Port::new("crucible", "game_socket_get_rtt");

pub const GAME_SOCKET_SEND_MSG: Port<GameSocketSendMsgArgs> =
    Port::new("crucible", "game_socket_send_msg");

pub const GAME_SOCKET_CANCEL_SEND_MSG: Port<GameSocketHandle> =
    Port::new("crucible", "game_socket_cancel_send_msg");

pub const GAME_SOCKET_RECV_MSG: Port<GameSocketHandle, Result<Vec<u8>, String>> =
    Port::new("crucible", "game_socket_recv_msg");

pub const GAME_SOCKET_OPEN_CHANNEL: Port<GameSocketHandle, GameSocketHandle> =
    Port::new("crucible", "game_socket_open_channel");

pub const GAME_SOCKET_CLOSE: Port<GameSocketHandle> = Port::new("crucible", "game_socket_close");

marshal_struct! {
    pub struct GameSocketSendMsgArgs {
        pub socket: GameSocketHandle,
        pub message: Vec<u8>,
        pub callback: fn(Result<(), String>),
    }
}

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd, Pod, Zeroable)]
#[repr(transparent)]
pub struct GameSocketHandle {
    pub raw: u32,
}

impl Marshal for GameSocketHandle {
    type Strategy = PodMarshal<Self>;
}
