use bytemuck::{Pod, Zeroable};
use wasmlink::{Marshal, PodMarshal, Port};

pub const GAME_SOCKET_GET_PING: Port<GameSocketHandle, Option<f64>> =
    Port::new("crucible", "game_socket_get_ping");

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd, Pod, Zeroable)]
#[repr(transparent)]
pub struct GameSocketHandle {
    pub raw: u32,
}

impl Marshal for GameSocketHandle {
    type Strategy = PodMarshal<Self>;
}
