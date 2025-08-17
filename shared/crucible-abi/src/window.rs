use wasmlink::{Port, marshal_struct, marshal_tagged_union};

use crate::{
    gpu::GpuTextureHandle,
    math::{DVec2, UVec2},
};

pub const WINDOW_REQUEST_REDRAW: Port<()> = Port::new("crucible", "window_request_redraw");

pub const WINDOW_BIND_HANDLERS: Port<WindowHandlers> =
    Port::new("crucible", "window_bind_handlers");

pub const WINDOW_UNBIND_HANDLERS: Port<()> = Port::new("crucible", "window_unbind_handlers");

marshal_struct! {
    pub struct WindowHandlers {
        pub redraw_requested: fn(RedrawRequestedArgs),
        pub mouse_moved: fn(DVec2),
        pub mouse_event: fn(MouseEvent),
        pub key_event: fn(KeyEvent),
        pub exit_requested: fn(()),
    }

    pub struct MouseEvent {
        pub button: MouseButton,
        pub pressed: bool,
    }

    pub struct KeyEvent {
        pub physical_key: Option<u32>,
        pub logical_key: LogicalKey,
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

marshal_tagged_union! {
    pub enum LogicalKey: u8 {
        Named(u32),
        Character(String),
        Unidentified(NativeKey),
        Dead(Option<char>),
    }

    pub enum NativeKey: u8 {
        Unidentified(()),
        Android(u32),
        MacOS(u16),
        Windows(u16),
        Xkb(u32),
        Web(String),
    }

    pub enum MouseButton : u8 {
        Left(()),
        Right(()),
        Middle(()),
        Back(()),
        Forward(()),
        Other(u16),
    }
}
