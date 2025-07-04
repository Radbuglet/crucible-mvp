use glam::DVec2;

use crate::base::run_loop::{MainLoopEvent, dispatch_event};

use super::keyboard::{Key, KeyCode, KeyEvent, KeyLocation, NamedKey, PhysicalKey};

// === Event Sink === //

#[derive(Debug)]
#[non_exhaustive]
pub enum ClientEvent {
    Redraw,
    MouseMoved(DVec2),
    KeyEvent(KeyEvent),
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

#[unsafe(no_mangle)]
unsafe extern "C" fn crucible_dispatch_key_event(
    physical_key: u32,
    logical_key_as_str: *mut u8,
    logical_key_as_str_len: usize,
    logical_key_as_named: u32,
    text: *mut u8,
    text_len: usize,
    location: u32,
    pressed: u32,
    repeat: u32,
) {
    dispatch_event(MainLoopEvent::Client(ClientEvent::KeyEvent(KeyEvent {
        physical_key: match KeyCode::from_winit(physical_key) {
            Some(code) => PhysicalKey::KeyCode(code),
            None => PhysicalKey::Unknown,
        },
        logical_key: if logical_key_as_str.is_null() {
            match NamedKey::from_winit(logical_key_as_named) {
                Some(named) => Key::Named(named),
                None => Key::Unknown,
            }
        } else {
            Key::Character(unsafe {
                String::from_raw_parts(
                    logical_key_as_str,
                    logical_key_as_str_len,
                    logical_key_as_str_len,
                )
            })
        },
        text: (!text.is_null())
            .then(|| unsafe { String::from_raw_parts(text, text_len, text_len) }),
        location: KeyLocation::from_winit(location).unwrap(),
        pressed: pressed != 0,
        repeat: repeat != 0,
    })));
}

// === Operations === //

pub fn request_redraw() {
    #[link(wasm_import_module = "crucible")]
    unsafe extern "C" {
        fn request_redraw();
    }

    unsafe { request_redraw() };
}
