use std::{net::SocketAddr, sync::Arc};

use anyhow::Context as _;
use arid::{Object, Strong, W};
use arid_entity::component;
use crucible_protocol::{
    codec::{DecodeCodec, EncodeCodec, FrameDecoder, FrameEncoder, recv_packet, send_packet},
    game,
};
use quinn::{
    crypto::rustls::QuicClientConfig,
    rustls::{
        self, DigitallySignedStruct, RootCertStore, SignatureScheme,
        client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
        crypto::{CryptoProvider, verify_tls12_signature, verify_tls13_signature},
        pki_types::{CertificateDer, ServerName, UnixTime},
    },
};
use tokio::net::ToSocketAddrs;
use tracing::{Instrument, info_span};

#[derive(Debug, Clone)]
pub enum CertValidationMode {
    DontAuthenticate,
    Pinned(CertificateDer<'static>),
    System,
}

#[derive(Debug)]
pub struct NetworkManager {}

component!(pub NetworkManager);

impl NetworkManagerHandle {
    pub fn new(w: W) -> Strong<Self> {
        tokio::spawn(
            async move {
                if let Err(err) = run_worker(CertValidationMode::DontAuthenticate).await {
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

async fn run_worker(cert_mode: CertValidationMode) -> anyhow::Result<()> {
    // Setup `rustls`
    let mut client_crypto = match cert_mode {
        CertValidationMode::DontAuthenticate => {
            // Adapted from: https://quinn-rs.github.io/quinn/quinn/certificate.html#insecure-connection
            #[derive(Debug)]
            struct SkipServerVerification(Arc<CryptoProvider>);

            impl Default for SkipServerVerification {
                fn default() -> Self {
                    Self(CryptoProvider::get_default().unwrap().clone())
                }
            }

            impl ServerCertVerifier for SkipServerVerification {
                fn verify_server_cert(
                    &self,
                    _end_entity: &CertificateDer<'_>,
                    _intermediates: &[CertificateDer<'_>],
                    _server_name: &ServerName<'_>,
                    _ocsp: &[u8],
                    _now: UnixTime,
                ) -> Result<ServerCertVerified, rustls::Error> {
                    Ok(ServerCertVerified::assertion())
                }
                fn verify_tls12_signature(
                    &self,
                    message: &[u8],
                    cert: &CertificateDer<'_>,
                    dss: &DigitallySignedStruct,
                ) -> Result<HandshakeSignatureValid, rustls::Error> {
                    verify_tls12_signature(
                        message,
                        cert,
                        dss,
                        &self.0.signature_verification_algorithms,
                    )
                }

                fn verify_tls13_signature(
                    &self,
                    message: &[u8],
                    cert: &CertificateDer<'_>,
                    dss: &DigitallySignedStruct,
                ) -> Result<HandshakeSignatureValid, rustls::Error> {
                    verify_tls13_signature(
                        message,
                        cert,
                        dss,
                        &self.0.signature_verification_algorithms,
                    )
                }

                fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
                    self.0.signature_verification_algorithms.supported_schemes()
                }

                fn requires_raw_public_keys(&self) -> bool {
                    false
                }

                fn root_hint_subjects(&self) -> Option<&[rustls::DistinguishedName]> {
                    None
                }
            }

            rustls::ClientConfig::builder()
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(SkipServerVerification::default()))
                .with_no_client_auth()
        }
        CertValidationMode::Pinned(pinned_cert) => {
            let mut certs = RootCertStore::empty();
            certs.add(pinned_cert)?;

            rustls::ClientConfig::builder()
                .with_root_certificates(certs)
                .with_no_client_auth()
        }
        CertValidationMode::System => {
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

    let (main_stream_tx, main_stream_rx) = conn.open_bi().await?;
    let mut main_stream_tx = FrameEncoder::new(main_stream_tx, EncodeCodec);
    let mut main_stream_rx = FrameDecoder::new(
        main_stream_rx,
        DecodeCodec {
            max_packet_size: u16::MAX as u32,
        },
    );

    tracing::info!("Sending server list request");

    send_packet(&mut main_stream_tx, game::SbHello1::ServerList).await?;

    tracing::info!("Sent server list request");

    let msg = recv_packet::<game::CbServerList1>(&mut main_stream_rx)
        .await?
        .context("never received server list")?;

    dbg!(msg);

    // We were the last peer to receive data so we can drop the socket immediately.

    Ok(())
}
