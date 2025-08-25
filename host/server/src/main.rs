use std::{net::SocketAddr, str::FromStr, sync::Arc};

use anyhow::Context;
use crucible_protocol::{
    codec::{DecodeCodec, EncodeCodec, FrameDecoder, FrameEncoder, recv_packet, send_packet},
    game,
};
use quinn::{
    VarInt,
    crypto::rustls::QuicServerConfig,
    rustls::{self, pki_types::PrivatePkcs8KeyDer},
};
use tracing::{Instrument, info_span, level_filters::LevelFilter};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .init();

    let bind_addr = SocketAddr::from_str("127.0.0.1:8080").unwrap();

    tracing::info!("Generating self-signed certificate");
    let cert = rcgen::generate_simple_self_signed(["localhost".to_string()])?;
    let key = PrivatePkcs8KeyDer::from(cert.signing_key.serialize_der());

    tracing::info!("Binding endpoint");

    let mut server_crypto = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert.cert.into()], key.into())?;

    server_crypto.alpn_protocols = vec![b"hq-29".to_vec()];

    let server_config =
        quinn::ServerConfig::with_crypto(Arc::new(QuicServerConfig::try_from(server_crypto)?));

    let endpoint = quinn::Endpoint::server(server_config, bind_addr)?;

    tracing::info!("Listening on {bind_addr}");

    let mut id_gen = 0u64;

    loop {
        let Some(conn) = endpoint.accept().await else {
            break;
        };

        let span = info_span!("socket process", id = id_gen);
        id_gen += 1;

        tokio::spawn(
            async move {
                tracing::info!(
                    "Got remote connection from address {}!",
                    conn.remote_address()
                );

                if let Err(err) = process_conn(conn).await {
                    tracing::error!("{err}");
                }
            }
            .instrument(span),
        );
    }

    Ok(())
}

async fn process_conn(conn: quinn::Incoming) -> anyhow::Result<()> {
    let conn = conn.accept()?.await?;

    tracing::info!("Accepted connection!");

    let (main_stream_tx, main_stream_rx) = conn.accept_bi().await?;
    let mut main_stream_tx = FrameEncoder::new(main_stream_tx, EncodeCodec);
    let mut main_stream_rx = FrameDecoder::new(
        main_stream_rx,
        DecodeCodec {
            max_packet_size: 64,
        },
    );

    let hello_packet = recv_packet::<game::SbHello1>(&mut main_stream_rx)
        .await?
        .context("no hello packet sent")?;

    match hello_packet {
        game::SbHello1::ServerList => {
            tracing::info!("Client wants server list");

            send_packet(
                &mut main_stream_tx,
                game::CbServerList1 {
                    motd: "Hello polynyan~".to_string(),
                    icon_png: Vec::new(),
                    content_server: None,
                    game_hash: blake3::Hash::from_bytes([0; blake3::OUT_LEN]),
                },
            )
            .await?;

            tracing::info!("Sent server list!");
        }
        game::SbHello1::Download => {
            tracing::info!("Client wants to download");
        }
        game::SbHello1::Play { game_hash } => {
            tracing::info!("Client wants to play game with hash {game_hash:?}");
        }
    }

    // TODO: This waits until the peer sends us the close packet or the peer times out. A
    //  malicious peer could spam packets at us because we don't receive them anymore and, although
    //  they'd eventually time-out, I'm worried that clients can use more memory than we may
    //  otherwise expect.
    conn.closed().await;

    tracing::info!("Connection closed");

    Ok(())
}
