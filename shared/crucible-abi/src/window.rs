use wasmlink::{Port, marshal_struct};

use crate::math::DVec2;

pub const WINDOW_REQUEST_REDRAW: Port<()> = Port::new("crucible", "window_request_redraw");

pub const WINDOW_BIND_HANDLERS: Port<WindowHandlers> =
    Port::new("crucible", "window_bind_handlers");

marshal_struct! {
    pub struct WindowHandlers {
        pub redraw_requested: fn(()),
        pub mouse_moved: fn(DVec2),
        pub key_event: fn(KeyEvent),
    }

    pub struct KeyEvent {
        // pub physical_key: PhysicalKey,
        // pub logical_key: Key,
        pub text: Option<String>,
        // pub location: KeyLocation,
        pub pressed: bool,
        pub repeat: bool,
    }
}
