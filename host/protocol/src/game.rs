use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SbHello1 {
    /// Transitions the socket to the `Ping` state. Replies immediately with a [`CbPingRes`] packet.
    /// Used to both keep the socket alive and measure latency.
    Ping,

    /// Replies with [`CbServerList1`] and closes the stream.
    ServerList,

    /// Replies with [`CbDownloadRes`], the payload if applicable, and closes the stream.
    Download { hash: blake3::Hash },

    /// Replies with [`CbPlayRes`] and then transparently . This can only happen once for a given connection.
    Play { game_hash: blake3::Hash },

    /// If `Play` has already been received in another stream, `PlayNewStream` will transition the
    /// current stream into
    PlayNewStream,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SbPingReq;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CbPingRes;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CbServerList1 {
    /// A message-of-the-day to display alongside the server listing.
    pub motd: String,

    /// A png-formatted icon for the server.
    pub icon_png: Vec<u8>,

    /// The dedicated content server from which the game's blob will be downloaded. `None` if the
    /// client should download the game from the server directly.
    pub content_server: Option<String>,

    /// The hash of the game's blob.
    pub content_hash: blake3::Hash,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CbDownloadRes {
    Found { content_len: u32 },
    NotFound,
    NotSupported,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CbPlayRes {
    Ready,
    WrongHash { expected: blake3::Hash },
}
