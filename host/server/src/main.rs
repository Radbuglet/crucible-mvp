use std::{net::SocketAddr, str::FromStr, sync::Arc};

use quinn::{
    crypto::rustls::QuicServerConfig,
    rustls::{self, pki_types::PrivatePkcs8KeyDer},
};
use tracing::{Instrument, info_span, level_filters::LevelFilter};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(LevelFilter::INFO.into()))
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

    loop {
        let Some(conn) = endpoint.accept().await else {
            break;
        };

        let span = info_span!("socket process", addr = conn.remote_address().to_string());

        tokio::spawn(
            async move {
                tracing::info!("Got remote connection!");

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

    let (main_stream_tx, main_stream_rx) = conn.accept_bi().await?;

    // TODO

    Ok(())
}
