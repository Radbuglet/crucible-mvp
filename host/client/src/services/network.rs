use std::{net::SocketAddr, sync::Arc};

use arid::{Object, Strong, W};
use arid_entity::component;
use quinn::{
    crypto::rustls::QuicClientConfig,
    rustls::{self, RootCertStore, pki_types::CertificateDer},
};
use tokio::net::ToSocketAddrs;
use tracing::{Instrument, info_span};

#[derive(Debug)]
pub struct NetworkManager {}

component!(pub NetworkManager);

impl NetworkManagerHandle {
    pub fn new(w: W) -> Strong<Self> {
        tokio::spawn(
            async move {
                if let Err(err) = run_worker(None).await {
                    tracing::error!("{err}");
                }
            }
            .instrument(info_span!("network client")),
        );

        NetworkManager {}.spawn(w)
    }

    pub fn connect(self, addr: impl ToSocketAddrs, w: W) {
        todo!()
    }
}

async fn run_worker(pinned: Option<CertificateDer<'static>>) -> anyhow::Result<()> {
    // Setup `rustls`
    let mut client_crypto = match pinned {
        Some(pinned_cert) => {
            let mut certs = RootCertStore::empty();
            certs.add(pinned_cert)?;

            rustls::ClientConfig::builder()
                .with_root_certificates(certs)
                .with_no_client_auth()
        }
        None => {
            use rustls_platform_verifier::ConfigVerifierExt as _;

            rustls::ClientConfig::with_platform_verifier()?
        }
    };

    client_crypto.alpn_protocols = vec![b"hq-29".to_vec()];

    let client_config =
        quinn::ClientConfig::new(Arc::new(QuicClientConfig::try_from(client_crypto)?));

    // Setup endpoint
    let mut endpoint = quinn::Endpoint::client("0.0.0.0:0".parse().unwrap())?;

    endpoint.set_default_client_config(client_config);

    // Connect to endpoint and perform login handshake.
    let conn = endpoint
        .connect("127.0.0.1:8080".parse::<SocketAddr>().unwrap(), "localhost")?
        .await?;

    let (mut main_stream_tx, main_stream_rx) = conn.open_bi().await?;

    main_stream_tx.write_all(b"hello everynyan~").await?;

    Ok(())
}
