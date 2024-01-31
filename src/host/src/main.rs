use anyhow::Context;

fn main() -> anyhow::Result<()> {
    let module_data = std::fs::read(std::env::args().nth(1).context("missing module path")?)?;

    // Construct core services
    let config = wasmtime::Config::new();
    let engine = wasmtime::Engine::new(&config)?;

    // Load the main module
    let module = wasmtime::Module::new(&engine, module_data)?;

    // Construct linker
    let mut linker = wasmtime::Linker::new(&engine);
    wasmtime_wasi::add_to_linker(&mut linker, |state| state)?;

    linker.func_wrap(
        "crucible0",
        "send_ipc",
        move |mut caller: wasmtime::Caller<'_, _>, start: u32, len: u32| {
            let data = caller
                .get_export("memory")
                .unwrap()
                .into_memory()
                .unwrap()
                .data(&caller);

            let bytes = Box::from_iter(data[start as usize..][..len as usize].iter().copied());
            drop(bytes);
        },
    )?;

    linker.func_wrap("crucible0", "do_stuff", move || {})?;

    // Construct instance
    let wasi_ctx = wasmtime_wasi::WasiCtxBuilder::new().inherit_stdio().build();
    let mut store = wasmtime::Store::new(&engine, wasi_ctx);

    let instance_1 = linker.instantiate(&mut store, &module)?;
    instance_1
        .get_typed_func::<(), ()>(&mut store, "_start")?
        .call(&mut store, ())?;

    Ok(())
}
