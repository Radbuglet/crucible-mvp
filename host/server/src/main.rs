use std::{env, net::SocketAddr, str::FromStr, sync::Arc};

use anyhow::Context as _;
use quinn::{
    crypto::rustls::QuicServerConfig,
    rustls::{self, crypto, pki_types::PrivatePkcs8KeyDer},
};
use tokio::fs;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;
use wasmall::encode::{SplitModuleArgs, split_module};

use crate::worker::{ContentConfig, GlobalState};

mod main_thread;
mod worker;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Setup logger
    tracing_subscriber::fmt::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .init();

    // Parse config
    let args = env::args().collect::<Vec<String>>();
    let args = args.iter().map(|v| v.as_str()).collect::<Vec<_>>();
    let args = args.as_slice();

    let [_bin_name, mod_path] = *args else {
        anyhow::bail!("invalid usage");
    };

    // Setup crypto
    crypto::aws_lc_rs::default_provider()
        .install_default()
        .ok()
        .context("failed to install AWS-LC crypto provider")?;

    // Setup endpoint
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

    // Create global state
    let archive = split_module(SplitModuleArgs {
        src: &fs::read(mod_path).await.context("failed to read module")?,
        truncate_relocations: true,
        truncate_debug: false,
    })?
    .archive;
    let archive = Arc::new(archive);
    let globals = Arc::new(GlobalState::new(ContentConfig::SelfHosted(archive)));

    // Run workers
    let listener = tokio::spawn(globals.listen(endpoint));

    // Run main thread
    // TODO

    listener.await??;

    Ok(())
}
