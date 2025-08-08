use bytemuck::{Pod, Zeroable};
use wasmlink::{Marshal, PodMarshal, Port, marshal_enum, marshal_struct};

// === Environment === //

pub const GET_RUN_MODE: Port<(), RunMode> = Port::new_hostbound("crucible", "get_run_mode");

pub const CONFIRM_APP_EXIT: Port<()> = Port::new_hostbound("crucible", "confirm_app_exit");

marshal_enum! {
    pub enum RunMode : u8 {
        Server,
        Client,
    }
}

// === Time === //

pub const GET_CURRENT_TIME: Port<(), f64> = Port::new_hostbound("crucible", "get_current_time");

pub const SPAWN_TIMEOUT: Port<SpawnTimeoutArgs, TimeoutHandle> =
    Port::new_hostbound("crucible", "spawn_timeout");

pub const CLEAR_TIMEOUT: Port<TimeoutHandle, ()> = Port::new_hostbound("crucible", "clear_timeout");

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

// === GPU Textures === //

pub const GPU_CREATE_TEXTURE: Port<GpuCreateTextureArgs, GpuTextureHandle> =
    Port::new_hostbound("crucible", "gpu_create_texture");

pub const GPU_CLEAR_TEXTURE: Port<GpuClearTextureArgs> =
    Port::new_hostbound("crucible", "gpu_clear_texture");

pub const GPU_UPLOAD_TEXTURE: Port<GpuUploadTextureArgs> =
    Port::new_hostbound("crucible", "gpu_upload_texture");

pub const GPU_DRAW_TEXTURE: Port<GpuDrawTextureArgs> =
    Port::new_hostbound("crucible", "gpu_draw_texture");

pub const GPU_DESTROY_TEXTURE: Port<GpuTextureHandle> =
    Port::new_hostbound("crucible", "gpu_destroy_texture");

marshal_struct! {
    pub struct GpuCreateTextureArgs {
        pub width: u32,
        pub height: u32,
    }

    pub struct GpuClearTextureArgs {
        pub handle: GpuTextureHandle,
        pub color: Bgra8Color,
    }

    pub struct GpuUploadTextureArgs {
        pub handle: GpuTextureHandle,
        pub buffer: Vec<Bgra8Color>,
        pub buffer_size: UVec2,
        pub at: UVec2,
        pub clip: Option<URect2>,
    }

    pub struct GpuDrawTextureArgs {
        pub dst_handle: GpuTextureHandle,
        pub src_handle: GpuTextureHandle,
        pub transform: Affine2,
        pub clip: UVec2,
        pub tint: Bgra8Color,
    }
}

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd, Pod, Zeroable)]
#[repr(transparent)]
pub struct GpuTextureHandle {
    pub raw: u32,
}

impl Marshal for GpuTextureHandle {
    type Strategy = PodMarshal<Self>;
}

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Pod, Zeroable)]
#[repr(C)]
pub struct Bgra8Color {
    pub b: u8,
    pub g: u8,
    pub r: u8,
    pub a: u8,
}

impl Marshal for Bgra8Color {
    type Strategy = PodMarshal<Self>;
}

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Pod, Zeroable)]
#[repr(C)]
pub struct UVec2 {
    pub x: u32,
    pub y: u32,
}

impl Marshal for UVec2 {
    type Strategy = PodMarshal<Self>;
}

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Pod, Zeroable)]
#[repr(C)]
pub struct URect2 {
    pub origin: UVec2,
    pub size: UVec2,
}

impl Marshal for URect2 {
    type Strategy = PodMarshal<Self>;
}

#[derive(Debug, Copy, Clone, PartialEq, Pod, Zeroable)]
#[repr(C)]
pub struct Affine2 {
    pub comps: [f64; 6],
}

impl Marshal for Affine2 {
    type Strategy = PodMarshal<Self>;
}

// === Windowing === //

// TODO
