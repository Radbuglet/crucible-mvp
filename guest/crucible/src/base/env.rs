#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub enum RunMode {
    Server,
    Client,
}

impl RunMode {
    pub fn get() -> Self {
        todo!()
    }

    pub fn is_server(self) -> bool {
        self == Self::Server
    }

    pub fn is_client(self) -> bool {
        self == Self::Client
    }
}

pub fn current_time() -> f64 {
    #[link(wasm_import_module = "crucible")]
    unsafe extern "C" {
        fn current_time() -> f64;
    }

    unsafe { current_time() }
}
