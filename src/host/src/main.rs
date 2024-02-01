use anyhow::Context;
use crucible_host::rt::marshal::MemoryExt;
use crucible_shared::DemoStructure;

fn main() -> anyhow::Result<()> {
    let module_data = std::fs::read(std::env::args().nth(1).context("missing module path")?)?;

    // Construct core services
    let config = wasmtime::Config::new();
    let engine = wasmtime::Engine::new(&config)?;

    // Load the main module
    let module = wasmtime::Module::new(&engine, module_data)?;

    // Construct linker
    struct StoreState {
        wasi: wasmtime_wasi::WasiCtx,
        main_memory: Option<wasmtime::Memory>,
    }

    let mut linker = wasmtime::Linker::<StoreState>::new(&engine);
    wasmtime_wasi::add_to_linker(&mut linker, |state| &mut state.wasi)?;

    linker.func_wrap(
        "crucible0",
        "read_my_struct",
        move |caller: wasmtime::Caller<'_, StoreState>, args: u32| {
            let data = caller.data().main_memory.unwrap();
            let data = data.data(&caller);

            let args = data.load_struct_raw::<DemoStructure>(args)?;
            let funnies = data.load_slice(args.funnies)?;

            dbg!(funnies);

            Ok(())
        },
    )?;

    // Construct instance
    let wasi_ctx = wasmtime_wasi::WasiCtxBuilder::new().inherit_stdio().build();
    let mut store = wasmtime::Store::new(
        &engine,
        StoreState {
            wasi: wasi_ctx,
            main_memory: None,
        },
    );

    let instance = linker.instantiate(&mut store, &module)?;
    store.data_mut().main_memory = instance
        .get_memory(&mut store, "memory")
        .context("failed to get main memory")?
        .into();

    instance
        .get_typed_func::<(), ()>(&mut store, "_start")?
        .call(&mut store, ())?;

    Ok(())
}
