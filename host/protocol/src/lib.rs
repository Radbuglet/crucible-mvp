//! Protocol definitions for Crucible, that is, message structures and utilities which are
//! shared between each protocol's server and client.
//!
//! There are four major protocols involved in Crucible networking:
//!
//! 1. The [`party`] server protocol, which uses a party server to manage rooms of friends. These
//!    handle party chat, send instructions on how to join a dedicated server as a party, and act
//!    as a WebRTC signalling server for setting up adhoc connections.
//! 2. The [`content`] server protocol, which describes the types of requests that can be used to
//!    download content. Content can be downloaded by either direct HTTP to a content server or
//!    embedded within the [`adhoc`] join protocol.
//! 3. The [`dedicated`] server game protocol, which is over QUIC.
//! 4. The [`adhoc`] server game protocol, which is over WebRTC data channels.
//!
//! Both [`dedicated`] and [`adhoc`] only describe the login procedure for each type of server. Once
//! the login process is complete, the sockets simply send over whatever the game binary wants to
//! send with appropriate framing and heartbeats.

pub mod adhoc;
pub mod content;
pub mod dedicated;
pub mod party;
