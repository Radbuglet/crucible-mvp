use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering::*},
    },
    time::{Duration, Instant},
};

use anyhow::Context as _;
use arid::{Handle, Object, Strong, W, Wr};
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
use tokio::{
    net::{ToSocketAddrs, lookup_host},
    sync::mpsc,
};
use tracing::{Instrument, info_span};

use crate::utils::promise::Promise;

// === Type Definitions === //

type NetworkPromise<T> = Promise<T, anyhow::Error>;

#[derive(Debug, Clone)]
pub enum CertValidationMode {
    DontAuthenticate,
    Pinned(CertificateDer<'static>),
    System,
}

// === GameSocket === //

#[derive(Debug)]
pub struct GameSocket {
    tx: mpsc::UnboundedSender<WorkerReq>,
    latency: Arc<AtomicU64>,
}

component!(pub GameSocket);

impl GameSocketHandle {
    pub fn new(
        endpoint: quinn::Endpoint,
        addr: impl 'static + Send + ToSocketAddrs,
        addr_name: impl Into<String>,
        validation_mode: CertValidationMode,
        connect_promise: NetworkPromise<()>,
        w: W,
    ) -> Strong<GameSocketHandle> {
        let addr_name = addr_name.into();
        let span = info_span!("net worker", name = addr_name.clone());

        let (tx, rx) = mpsc::unbounded_channel();
        let latency = Arc::new(AtomicU64::new(f64::NAN.to_bits()));

        let worker = Worker {
            endpoint,
            addr_name,
            validation_mode,
            latency: latency.clone(),
            rx,
            connect_promise,
        };

        tokio::spawn(
            async move {
                if let Err(err) = worker.run(addr).await {
                    tracing::error!("{err:?}");
                }
            }
            .instrument(span),
        );

        GameSocket { tx, latency }.spawn(w)
    }

    pub fn latency(self, w: Wr) -> Option<f64> {
        let latency = f64::from_bits(self.r(w).latency.load(Relaxed));

        (!latency.is_nan()).then_some(latency)
    }

    pub fn info(self, callback: NetworkPromise<game::CbServerList1>, w: Wr) {
        _ = self.r(w).tx.send(WorkerReq::GetInfo { callback });
    }
}

// === Worker === //

struct Worker {
    endpoint: quinn::Endpoint,
    addr_name: String,
    validation_mode: CertValidationMode,
    latency: Arc<AtomicU64>,
    rx: mpsc::UnboundedReceiver<WorkerReq>,
    connect_promise: NetworkPromise<()>,
}

#[derive(Debug)]
enum WorkerReq {
    GetInfo {
        callback: NetworkPromise<game::CbServerList1>,
    },
    Download {
        hash: blake3::Hash,
        callback: NetworkPromise<()>,
    },
}

impl Worker {
    async fn run(self, addr: impl ToSocketAddrs) -> anyhow::Result<()> {
        let Self {
            endpoint,
            addr_name,
            validation_mode: cert_mode,
            latency,
            mut rx,
            connect_promise,
        } = self;

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

        // Connect to endpoint and perform login handshake.
        let addr = lookup_host(addr)
            .await?
            .next()
            .context("no server address found")?;

        tracing::info!("connecting to {addr:?}");

        let conn = endpoint
            .connect_with(client_config, addr, &addr_name)?
            .await?;

        // Start ping task
        tokio::spawn({
            let conn = conn.clone();

            async move {
                if let Err(err) = Self::process_ping(conn, latency).await {
                    match err.downcast_ref::<quinn::ConnectionError>() {
                        Some(
                            quinn::ConnectionError::ApplicationClosed(_)
                            | quinn::ConnectionError::ConnectionClosed(_),
                        ) => {
                            // (fallthrough)
                        }
                        _ => {
                            tracing::error!("ping task crashed: {err}");
                        }
                    }
                }
            }
            .in_current_span()
        });

        // Start main loop
        tracing::info!("connected to remote host");

        connect_promise.resolve(());

        let mut task_counter = 0;

        while let Some(cmd) = rx.recv().await {
            let conn = conn.clone();

            tokio::spawn(
                async move {
                    tracing::info!("processing {cmd:?}");

                    match cmd {
                        WorkerReq::GetInfo { callback } => {
                            callback.finish(Self::process_get_info(conn).await);
                        }
                        WorkerReq::Download { hash, callback } => todo!(),
                    }
                }
                .instrument(info_span!("task worker", task = task_counter)),
            );

            task_counter += 1;
        }

        tracing::info!("closed connection");

        Ok(())
    }

    async fn process_ping(conn: quinn::Connection, latency: Arc<AtomicU64>) -> anyhow::Result<()> {
        let (stream_tx, stream_rx) = conn.open_bi().await?;
        let mut stream_tx = FrameEncoder::new(stream_tx, EncodeCodec);
        let mut stream_rx = FrameDecoder::new(
            stream_rx,
            DecodeCodec {
                max_packet_size: u16::MAX as u32,
            },
        );

        let mut last_send = Instant::now();

        // Transition to the ping state, effectively sending out a ping request.
        send_packet(&mut stream_tx, game::SbHello1::Ping).await?;

        loop {
            // Wait for pong.
            if recv_packet::<game::CbPingRes>(&mut stream_rx)
                .await?
                .is_none()
            {
                break;
            }

            // Write out latency.
            latency.store(last_send.elapsed().as_secs_f64().to_bits(), Relaxed);

            // Wait for a new period.
            tokio::time::sleep(Duration::from_millis(1000)).await;

            // Send a new ping.
            last_send = Instant::now();
            send_packet(&mut stream_tx, game::SbPingReq).await?;
        }

        Ok(())
    }

    async fn process_get_info(conn: quinn::Connection) -> anyhow::Result<game::CbServerList1> {
        let (stream_tx, stream_rx) = conn.open_bi().await?;

        let mut stream_tx = FrameEncoder::new(stream_tx, EncodeCodec);
        let mut stream_rx = FrameDecoder::new(
            stream_rx,
            DecodeCodec {
                max_packet_size: u16::MAX as u32,
            },
        );

        send_packet(&mut stream_tx, game::SbHello1::ServerList).await?;

        let info = recv_packet::<game::CbServerList1>(&mut stream_rx)
            .await?
            .context("no server list sent")?;

        Ok(info)
    }
}
