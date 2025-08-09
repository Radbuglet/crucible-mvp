use std::sync::Once;

pub fn setup_logger() {
    static ONCE: Once = Once::new();

    ONCE.call_once(|| {
        // console_error_panic_hook::set_once();
        // tracing_wasm::set_as_global_default();

        // TODO
    });
}
