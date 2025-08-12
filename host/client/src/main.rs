#![allow(clippy::single_match)]

use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;

mod app;
mod bindings;
mod services;
mod utils;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(LevelFilter::INFO.into()))
        .init();

    app::run_app()?;

    Ok(())
}
