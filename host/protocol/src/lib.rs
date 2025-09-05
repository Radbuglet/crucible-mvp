//! Protocol definitions for Crucible, that is, message structures and utilities which are
//! shared between each protocol's server and client.
//!
//! There are four major protocols involved in Crucible networking:
//!
//! 1. The [`party`] server protocol over QUIC, which uses a party server to manage rooms of
//!    friends. These handle party chat, send instructions on how to join a dedicated server as a
//!    party, and act as a signalling server for setting up peer-to-peer connections.
//! 2. The [`content`] server protocol, which describes the types of requests that can be used to
//!    download content from a dedicated content server.
//! 3. The [`game`] server protocol, which is over QUIC. This handles both ad-hoc and dedicated
//!    servers. This only describes the login procedure for each type of server. Once
//!    the login process is complete, the sockets simply send over whatever the game binary wants to
//!    send with appropriate framing and heartbeats.

pub mod codec;
pub mod content;
pub mod game;
pub mod party;
