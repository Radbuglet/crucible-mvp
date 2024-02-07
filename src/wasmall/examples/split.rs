use anyhow::Context;
use wasmall::splitter::split_module;

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let code = std::fs::read(std::env::args().nth(1).context("missing path")?)?;
    split_module(&code)
}
