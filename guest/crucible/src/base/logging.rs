use std::sync::Once;

use crucible_abi as abi;
use wasmlink::{GuestStrRef, bind_port};

bind_port! {
    fn [abi::LOG_MESSAGE] "crucible".log_message(abi::MessageLogArgs);
}

pub fn setup_logger() {
    static ONCE: Once = Once::new();

    ONCE.call_once(|| {
        // console_error_panic_hook::set_once();
        // tracing_wasm::set_as_global_default();

        log_message(&abi::MessageLogArgs {
            msg: GuestStrRef::new("Funny little message :)"),
            file: GuestStrRef::new("made_up_file.rs"),
            module: GuestStrRef::new("made_up::module"),
            line: 42,
            column: 10,
            level: abi::MessageLogLevel::Panic,
        });

        // TODO
    });
}
