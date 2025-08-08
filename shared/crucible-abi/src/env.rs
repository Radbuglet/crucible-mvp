use bytemuck::{Pod, Zeroable};
use wasmlink::{Marshal, PodMarshal, Port, marshal_enum, marshal_struct};

// === Environment === //

pub const GET_RUN_MODE: Port<(), RunMode> = Port::new("crucible", "get_run_mode");

pub const CONFIRM_APP_EXIT: Port<()> = Port::new("crucible", "confirm_app_exit");

marshal_enum! {
    pub enum RunMode : u8 {
        Server,
        Client,
    }
}

// === Time === //

pub const GET_CURRENT_TIME: Port<(), f64> = Port::new("crucible", "get_current_time");

pub const SPAWN_TIMEOUT: Port<SpawnTimeoutArgs, TimeoutHandle> =
    Port::new("crucible", "spawn_timeout");

pub const CLEAR_TIMEOUT: Port<TimeoutHandle, ()> = Port::new("crucible", "clear_timeout");

marshal_struct! {
    pub struct SpawnTimeoutArgs {
        pub handler: fn(()),
        pub expires_at: f64,
    }
}

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd, Pod, Zeroable)]
#[repr(transparent)]
pub struct TimeoutHandle {
    pub raw: u32,
}

impl Marshal for TimeoutHandle {
    type Strategy = PodMarshal<Self>;
}

// === Logging === //

pub const LOG_MESSAGE: Port<MessageLogArgs> = Port::new("crucible", "log_message");

marshal_struct! {
    pub struct MessageLogArgs {
        pub msg: String,
        pub file: String,
        pub module: String,
        pub line: u32,
        pub column: u32,
        pub level: MessageLogLevel,
    }
}

marshal_enum! {
    pub enum MessageLogLevel : u8 {
        Trace,
        Debug,
        Info,
        Warn,
        Error,
        Fatal,
    }
}
