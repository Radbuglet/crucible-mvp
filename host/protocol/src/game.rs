//! To connect to a dedicated host, the client first connects to the server to request its server
//! list information. The server quickly returns, among other things, the game's hash and a URL to
//! the content server and disconnects the client. The client can then download the content from the
//! content server without being connected to the game server. After that process is done, the
//! client can reconnect to the dedicated server and send the `Play` packet with the hash of the
//! game it just downloaded. After that, the protocol forwards user-generated packets and
//! occasionally sends heartbeats to keep the QUIC connection alive.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SbHello1 {
    /// Transitions the connection into the `ServerList` state.
    ServerList,

    /// Transitions the connection into the `Download` state governed by the
    /// [content](crate::content) protocol.
    Download,

    /// If the game hash is correct, transitions the connection into the `Play` state and
    /// transparently forwards packets.
    Play { game_hash: blake3::Hash },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CbServerList1 {
    pub motd: String,
    pub icon_png: Vec<u8>,
    pub content_server: Option<String>,
    pub game_hash: blake3::Hash,
}
