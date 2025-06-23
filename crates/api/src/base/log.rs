#[derive(Debug, Copy, Clone)]
pub enum LogLevel {
    Trace = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
    Fatal = 5,
}

pub fn log_str(level: LogLevel, msg: &str) {
    #[link(wasm_import_module = "crucible")]
    unsafe extern "C" {
        fn log(level: u32, msg: *const u8, len: usize);
    }

    unsafe { log(level as u32, msg.as_ptr(), msg.len()) }
}
