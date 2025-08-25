#![allow(clippy::single_match)]

use anyhow::Context;
use quinn::rustls::crypto;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;

mod app;
mod bindings;
mod services;
mod utils;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .init();

    // By default, `rustls` will automatically select a `CryptoProvider` if either the `aws-lc-rs`
    //  or `ring` features are enabled. This is convenient for prototyping but is also very brittle
    //  as hidden dependencies can enable `rustls` feature flags unexpectedly. Additionally, not all
    //  consumers call `CryptoProvider`'s internal `get_default_or_install_from_crate_features`
    //  function before fetching a provider. To avoid all these headaches, we choose our default
    //  provider early and explicitly.
    crypto::aws_lc_rs::default_provider()
        .install_default()
        .ok()
        .context("failed to install AWS-LC crypto provider")?;

    let rt = tokio::runtime::Runtime::new()?;
    let _guard = rt.enter();

    app::run_app()?;

    Ok(())
}
