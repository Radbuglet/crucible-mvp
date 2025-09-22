use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering::*},
    },
    time::{Duration, Instant},
};

use anyhow::Context as _;
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

use crate::{
    app::App,
    utils::{
        sync::{Promise, PromiseReceiver, promise},
        winit::BackgroundTasks,
    },
};

// === Type Definitions === //

type NetworkPromise<T> = Promise<T, anyhow::Error>;
type NetworkPromiseRx<T> = PromiseReceiver<T, anyhow::Error>;

#[derive(Debug, Clone)]
pub enum CertValidationMode {
    DontAuthenticate,
    Pinned(CertificateDer<'static>),
    System,
}

// === LoginSocket === //

#[derive(Debug)]
pub struct LoginSocket {
    req_tx: mpsc::UnboundedSender<WorkerReq>,
    rtt: Arc<AtomicU64>,
}

impl LoginSocket {
    pub async fn new(
        background: BackgroundTasks<App>,
        endpoint: quinn::Endpoint,
        addr: impl 'static + Send + ToSocketAddrs,
        addr_name: impl Into<String>,
        validation_mode: CertValidationMode,
    ) -> anyhow::Result<Self> {
        let addr_name = addr_name.into();
        let span = info_span!("net worker", name = addr_name.clone());

        let (req_tx, req_rx) = mpsc::unbounded_channel();
        let (connect_tx, connect_rx) = promise();
        let rtt = Arc::new(AtomicU64::new(f64::NAN.to_bits()));

        let worker = WorkerArgs {
            background: background.clone(),
            endpoint,
            addr_name,
            validation_mode,
            rtt: rtt.clone(),
            req_rx,
            connect_promise: connect_tx,
        };

        background.spawn(
            async move {
                if let Err(err) = run_worker(worker, addr).await {
                    tracing::error!("{err:?}");
                }
            }
            .instrument(span),
        );

        connect_rx.recv().await?;

        Ok(Self { req_tx, rtt })
    }

    pub fn rtt(&self) -> Option<f64> {
        let rtt = f64::from_bits(self.rtt.load(Relaxed));

        (!rtt.is_nan()).then_some(rtt)
    }

    pub fn info(&self) -> NetworkPromiseRx<game::CbServerList1> {
        let (tx, rx) = promise();
        _ = self.req_tx.send(WorkerReq::GetInfo { callback: tx });
        rx
    }

    pub fn play(
        &self,
        game_hash: blake3::Hash,
    ) -> NetworkPromiseRx<Result<GameSocket, blake3::Hash>> {
        let (tx, rx) = promise();
        _ = self.req_tx.send(WorkerReq::Play {
            game_hash,
            callback: tx,
        });
        rx
    }
}

// === GameSocket === //

#[derive(Debug)]
pub struct GameSocket {}

impl GameSocket {
    pub fn try_send_msg(&self, data: &[u8]) {
        todo!()
    }
}

// === Worker === //

struct WorkerArgs {
    background: BackgroundTasks<App>,
    endpoint: quinn::Endpoint,
    addr_name: String,
    validation_mode: CertValidationMode,
    rtt: Arc<AtomicU64>,
    req_rx: mpsc::UnboundedReceiver<WorkerReq>,
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
    Play {
        game_hash: blake3::Hash,
        callback: NetworkPromise<Result<GameSocket, blake3::Hash>>,
    },
}

async fn run_worker(args: WorkerArgs, addr: impl ToSocketAddrs) -> anyhow::Result<()> {
    let WorkerArgs {
        background,
        endpoint,
        addr_name,
        validation_mode: cert_mode,
        rtt,
        mut req_rx,
        connect_promise,
    } = args;

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
    background.spawn({
        let conn = conn.clone();

        async move {
            if let Err(err) = process_ping(conn, rtt).await {
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

    connect_promise.accept(());

    let mut task_counter = 0;
    let mut play_socket_counter = 0;
    let hash_already_verified = Arc::new(AtomicBool::new(false));

    while let Some(cmd) = req_rx.recv().await {
        let conn = conn.clone();
        let hash_already_verified = hash_already_verified.clone();

        background.spawn({
            let background = background.clone();

            async move {
                tracing::info!("processing {cmd:?}");

                match cmd {
                    WorkerReq::GetInfo { callback } => {
                        callback.finish(process_get_info(conn).await);
                    }
                    WorkerReq::Download { hash, callback } => todo!(),
                    WorkerReq::Play {
                        game_hash,
                        callback,
                    } => {
                        background.spawn(process_play(PlayArgs {
                            id: play_socket_counter,
                            conn,
                            game_hash,
                            hash_already_verified,
                            callback,
                        }));
                    }
                }
            }
            .instrument(info_span!("task worker", task = task_counter))
        });

        task_counter += 1;
        play_socket_counter += 1;
    }

    tracing::info!("closed connection");

    Ok(())
}

async fn process_ping(conn: quinn::Connection, rtt: Arc<AtomicU64>) -> anyhow::Result<()> {
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

        // Write out RTT.
        rtt.store(last_send.elapsed().as_secs_f64().to_bits(), Relaxed);

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

struct PlayArgs {
    id: u64,
    conn: quinn::Connection,
    game_hash: blake3::Hash,
    hash_already_verified: Arc<AtomicBool>,
    callback: NetworkPromise<Result<GameSocket, blake3::Hash>>,
}

async fn process_play(args: PlayArgs) {
    let PlayArgs {
        id,
        conn,
        game_hash,
        hash_already_verified,
        callback,
    } = args;

    // Request the channel from the peer
    fn infer_helper<R, F: Future<Output = anyhow::Result<R>>>(f: F) -> F {
        f
    }

    let res = infer_helper(async {
        let (stream_tx, stream_rx) = conn.open_bi().await?;

        let mut stream_tx = FrameEncoder::new(stream_tx, EncodeCodec);
        let mut stream_rx = FrameDecoder::new(
            stream_rx,
            DecodeCodec {
                max_packet_size: u16::MAX as u32,
            },
        );

        if !hash_already_verified.load(Relaxed) {
            send_packet(
                &mut stream_tx,
                game::SbHello1::PlayChecked { game_hash, id },
            )
            .await?;

            let info = recv_packet::<game::CbPlayRes>(&mut stream_rx)
                .await?
                .context("no play response sent")?;

            match info {
                game::CbPlayRes::Ready => {
                    // (fallthrough)
                }
                game::CbPlayRes::WrongHash { expected } => {
                    return Ok(Err(expected));
                }
            }

            hash_already_verified.store(true, Relaxed);
        } else {
            send_packet(&mut stream_tx, game::SbHello1::PlayUnchecked { id }).await?;
        }

        Ok(Ok((stream_tx, stream_rx)))
    })
    .await;

    let (mut stream_tx, mut stream_rx) = match res {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => {
            callback.accept(Err(e));
            return;
        }
        Err(err) => {
            callback.reject(err);
            return;
        }
    };

    callback.accept(Ok(GameSocket {}));

    // Process messages
    // TODO
}
