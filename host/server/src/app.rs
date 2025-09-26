use std::{env, net::SocketAddr, rc::Rc, str::FromStr, sync::Arc};

use anyhow::Context as _;
use arid::World;
use crucible_host_shared::lang;
use quinn::{
    crypto::rustls::QuicServerConfig,
    rustls::{self, crypto, pki_types::PrivatePkcs8KeyDer},
};
use wasmall::encode::{SplitModuleArgs, split_module};

use crate::worker::{ContentConfig, GlobalState};

pub type BackgroundTasks = lang::BackgroundTasks<(), App>;

#[derive(Debug)]
pub struct App {
    pub world: World,
}

pub async fn main_task(background: BackgroundTasks) -> anyhow::Result<()> {
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
        src: &smol::fs::read(mod_path)
            .await
            .context("failed to read module")?,
        truncate_relocations: true,
        truncate_debug: false,
    })?
    .archive;
    let archive = Rc::new(archive);
    let globals = Rc::new(GlobalState::new(
        background.clone(),
        ContentConfig::SelfHosted(archive),
    ));

    // Run workers
    let listener = background.spawn(globals.listen(endpoint));

    // Run main thread
    // TODO

    listener.await?;

    Ok(())
}
