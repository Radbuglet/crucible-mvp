use wasmlink::{Port, marshal_struct};

use crate::{
    gpu::GpuTextureHandle,
    math::{DVec2, UVec2},
};

pub const WINDOW_REQUEST_REDRAW: Port<()> = Port::new("crucible", "window_request_redraw");

pub const WINDOW_BIND_HANDLERS: Port<WindowHandlers> =
    Port::new("crucible", "window_bind_handlers");

marshal_struct! {
    pub struct WindowHandlers {
        pub redraw_requested: fn(RedrawRequestedArgs),
        pub mouse_moved: fn(DVec2),
        pub key_event: fn(KeyEvent),
    }

    pub struct KeyEvent {
        pub physical_key: u32,
        pub logical_key: u32,
        pub text: Option<String>,
        pub location: u32,
        pub pressed: bool,
        pub repeat: bool,
    }

    pub struct RedrawRequestedArgs {
        pub fb: GpuTextureHandle,
        pub size: UVec2,
    }
}
