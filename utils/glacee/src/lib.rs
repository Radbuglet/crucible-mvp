//! An implementation of the Internet Connectivity Establishment protocol (ICE, [rfc8445]) for use
//! in Crucible.
//!
//! This crate's name should technically be "Glacée."
//!
//! Compared to an entire WebRTC stack, this crate only implements the lowest layer of the stack,
//! providing a best-effort mechanism for obtaining a UDP socket between two peers. It does not
//! implement the [DTLS] encryption layer, nor the [SCTP] protocol used by data channels, nor the
//! (S)[RTP] protocol used by media channels. Additionally, this crate does not implement [TURN] for
//! when a direct UDP socket cannot be obtained. Instead, the user is expected to use a protocol
//! like [QUIC] to connect the two peers.
//!
//! Currently, this is just a wrapper around [`rice-proto`] which is can interop with `tokio`.
//! Ideally, however, we'd like to eventually adopt [some techniques from Tailscale] to achieve
//! better connectivity.
//!
//! [rfc8445]: https://datatracker.ietf.org/doc/html/rfc8445
//! [DTLS]: https://datatracker.ietf.org/doc/html/rfc5764
//! [SCTP]: https://datatracker.ietf.org/doc/html/rfc4960
//! [RTP]: https://datatracker.ietf.org/doc/html/rfc3550
//! [TURN]: https://datatracker.ietf.org/doc/html/rfc5766
//! [QUIC]: https://datatracker.ietf.org/doc/rfc9000/
//! [`rice-proto`]: https://docs.rs/rice-proto/latest/rice_proto/
//! [some techniques from Tailscale]: https://tailscale.com/blog/how-nat-traversal-works
