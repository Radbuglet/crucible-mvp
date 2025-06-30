use glam::DVec2;

use crate::base::run_loop::{MainLoopEvent, dispatch_event};

// === Event Sink === //

#[derive(Debug)]
pub enum ClientEvent {
    Redraw,
    MouseMoved(DVec2),
}

#[unsafe(no_mangle)]
extern "C" fn crucible_dispatch_redraw() {
    dispatch_event(MainLoopEvent::Client(ClientEvent::Redraw));
}

#[unsafe(no_mangle)]
extern "C" fn crucible_dispatch_mouse_moved(x: f64, y: f64) {
    dispatch_event(MainLoopEvent::Client(ClientEvent::MouseMoved(DVec2::new(
        x, y,
    ))));
}

// === Operations === //

pub fn request_redraw() {
    #[link(wasm_import_module = "crucible")]
    unsafe extern "C" {
        fn request_redraw();
    }

    unsafe { request_redraw() };
}
