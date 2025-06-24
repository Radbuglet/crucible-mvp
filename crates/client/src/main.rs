use std::{env, fs, sync::Arc};

use anyhow::Context;
use runtime::{
    base::{MainMemory, RtModule, RtState},
    log::RtLogger,
};

mod runtime;

fn main() -> anyhow::Result<()> {
    let engine = wasmtime::Engine::new(&wasmtime::Config::new())?;

    // Load the module
    let module = env::args()
        .nth(1)
        .context("missing path to target module")?;

    let module =
        fs::read(&module).with_context(|| format!("failed to read module at {module:?}"))?;

    let module = wasmtime::Module::new(&engine, &module).context("failed to load module")?;

    // Create a linker
    let mut linker = wasmtime::Linker::new(&engine);
    RtLogger::define(&mut linker)?;

    // Setup instance
    let mut store = wasmtime::Store::new(&engine, RtState::new());

    let instance = linker
        .instantiate(&mut store, &module)
        .context("failed to instantiate module")?;

    MainMemory::init(&mut store, instance)?;
    RtLogger::init(
        &mut store,
        Arc::new(move |_store, msg| {
            eprintln!("{msg}");
        }),
    )?;

    let main = instance
        .get_typed_func::<(u32, u32), u32>(&mut store, "main")
        .context("failed to get main function")?;

    dbg!(main.call(&mut store, (0, 0))?);

    Ok(())
}
