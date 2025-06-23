use std::{env, fs};

use anyhow::Context;

#[derive(Debug, Default)]
struct StoreState {
    main_memory: Option<wasmtime::Memory>,
}

impl StoreState {
    fn main_memory(&self) -> wasmtime::Memory {
        self.main_memory.unwrap()
    }
}

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
    linker.func_wrap(
        "crucible",
        "log",
        |mut caller: wasmtime::Caller<'_, StoreState>,
         level: u32,
         base: u32,
         len: u32|
         -> anyhow::Result<()> {
            let data = caller.data().main_memory().data_mut(&mut caller);

            let msg = data
                .get(base as usize..)
                .and_then(|v| v.get(..len as usize))
                .context("failed to get message")?;

            let msg = std::str::from_utf8(msg).context("malformed log message")?;

            eprintln!("{msg}");

            Ok(())
        },
    )?;

    let mut store = wasmtime::Store::new(&engine, StoreState::default());
    let instance = linker
        .instantiate(&mut store, &module)
        .context("failed to instantiate module")?;

    let main_memory = instance
        .get_memory(&mut store, "memory")
        .context("failed to get main memory")?;

    store.data_mut().main_memory = Some(main_memory);

    let main = instance
        .get_typed_func::<(u32, u32), u32>(&mut store, "main")
        .context("failed to get main function")?;

    dbg!(main.call(&mut store, (0, 0))?);

    Ok(())
}
