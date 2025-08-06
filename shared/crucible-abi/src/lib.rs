use bytemuck::{Pod, Zeroable};
use wasmlink::{Marshal, PodMarshal, Port, marshal_enum, marshal_struct};

pub const GET_CURRENT_TIME: Port<(), f64> = Port::new_hostbound("crucible", "get_current_time");

pub const GET_RUN_MODE: Port<(), RunMode> = Port::new_hostbound("crucible", "get_run_mode");

pub const CONFIRM_APP_EXIT: Port<()> = Port::new_hostbound("crucible", "confirm_app_exit");

marshal_enum! {
    pub enum RunMode : u8 {
        Server,
        Client,
    }
}

pub const LOG_MESSAGE: Port<MessageLogArgs> = Port::new_hostbound("crucible", "log_message");

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

pub const GPU_CREATE_TEXTURE: Port<GpuCreateTextureArgs, GpuHandle> =
    Port::new_hostbound("crucible", "gpu_create_texture");

pub const GPU_CLEAR_TEXTURE: Port<GpuClearTextureArgs> =
    Port::new_hostbound("crucible", "gpu_clear_texture");

marshal_struct! {
    pub struct GpuCreateTextureArgs {
        pub width: u32,
        pub height: u32,
    }

    pub struct GpuClearTextureArgs {
        pub handle: GpuHandle,
        pub color: BgraColor,
    }
}

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd, Pod, Zeroable)]
#[repr(transparent)]
pub struct GpuHandle {
    pub raw: u32,
}

impl Marshal for GpuHandle {
    type Strategy = PodMarshal<Self>;
}

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Pod, Zeroable)]
#[repr(C)]
pub struct BgraColor {
    pub b: u8,
    pub g: u8,
    pub r: u8,
    pub a: u8,
}

impl Marshal for BgraColor {
    type Strategy = PodMarshal<Self>;
}
