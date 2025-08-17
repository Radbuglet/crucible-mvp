use std::sync::atomic::{AtomicBool, Ordering::Relaxed};

use futures::{StreamExt, channel::mpsc};
use glam::DVec2;
use wasmlink::{OwnedGuestClosure, bind_port};

use crate::{
    base::{env::RunMode, task::wake_executor},
    gfx::texture::GpuTexture,
    window::keyboard::{Key, KeyCode, KeyLocation, NamedKey, PhysicalKey},
};

use super::keyboard::KeyEvent;

// === Window === //

static HAS_WINDOW_SINGLETON: AtomicBool = AtomicBool::new(false);

#[derive(Debug)]
pub struct Window {
    rx: mpsc::UnboundedReceiver<WindowEvent>,
    _redraw_requested: OwnedGuestClosure<crucible_abi::RedrawRequestedArgs>,
    _mouse_moved: OwnedGuestClosure<crucible_abi::DVec2>,
    _key_event: OwnedGuestClosure<crucible_abi::KeyEvent>,
    _exit_requested: OwnedGuestClosure<()>,
}

#[derive(Debug)]
#[non_exhaustive]
pub enum WindowEvent {
    Redraw(GpuTexture),
    MouseMoved(DVec2),
    KeyEvent(KeyEvent),
    ExitRequested,
}

impl Window {
    pub fn acquire() -> Self {
        RunMode::get().assert_client();

        assert!(
            HAS_WINDOW_SINGLETON
                .compare_exchange(false, true, Relaxed, Relaxed)
                .is_ok(),
            "`Window` singleton already acquired"
        );

        bind_port! {
            fn [crucible_abi::WINDOW_BIND_HANDLERS] "crucible".window_bind_handlers(crucible_abi::WindowHandlers);
        }

        let (tx, rx) = mpsc::unbounded();

        let redraw_requested = OwnedGuestClosure::<crucible_abi::RedrawRequestedArgs>::new({
            let tx = tx.clone();

            move |arg| {
                tx.unbounded_send(WindowEvent::Redraw(GpuTexture {
                    handle: arg.fb,
                    size: bytemuck::cast(arg.size),
                }))
                .unwrap();
                wake_executor();
            }
        });

        let mouse_moved = OwnedGuestClosure::<crucible_abi::DVec2>::new({
            let tx = tx.clone();

            move |arg| {
                tx.unbounded_send(WindowEvent::MouseMoved(bytemuck::cast(arg)))
                    .unwrap();
                wake_executor();
            }
        });

        let key_event = OwnedGuestClosure::<crucible_abi::KeyEvent>::new({
            let tx = tx.clone();

            move |arg| {
                tx.unbounded_send(WindowEvent::KeyEvent(KeyEvent {
                    physical_key: match arg.physical_key.decode() {
                        Some(v) => PhysicalKey::KeyCode(KeyCode::from_winit(v).unwrap()),
                        None => PhysicalKey::Unknown,
                    },
                    logical_key: match (
                        arg.logical_key.named.decode(),
                        arg.logical_key.character.decode(),
                    ) {
                        (None, None) => Key::Unknown,
                        (None, Some(v)) => Key::Character(v.decode()),
                        (Some(v), None) => Key::Named(NamedKey::from_winit(v).unwrap()),
                        (Some(_), Some(_)) => unreachable!(),
                    },
                    text: arg.text.decode().map(|v| v.decode()),
                    location: KeyLocation::from_winit(arg.location).unwrap(),
                    pressed: arg.pressed,
                    repeat: arg.repeat,
                }))
                .unwrap();

                wake_executor();
            }
        });

        let exit_requested = OwnedGuestClosure::<()>::new({
            let tx = tx.clone();

            move |()| {
                tx.unbounded_send(WindowEvent::ExitRequested).unwrap();
                wake_executor();
            }
        });

        window_bind_handlers(&crucible_abi::WindowHandlers {
            redraw_requested: redraw_requested.handle(),
            mouse_moved: mouse_moved.handle(),
            key_event: key_event.handle(),
            exit_requested: exit_requested.handle(),
        });

        Self {
            rx,
            _redraw_requested: redraw_requested,
            _mouse_moved: mouse_moved,
            _key_event: key_event,
            _exit_requested: exit_requested,
        }
    }

    pub fn request_redraw(&mut self) {
        bind_port! {
            fn [crucible_abi::WINDOW_REQUEST_REDRAW] "crucible".window_request_redraw(());
        }

        window_request_redraw(&());
    }

    pub async fn next_event(&mut self) -> WindowEvent {
        self.rx.next().await.unwrap()
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        bind_port! {
            fn [crucible_abi::WINDOW_UNBIND_HANDLERS] "crucible".window_unbind_handlers(());
        }

        window_unbind_handlers(&());

        HAS_WINDOW_SINGLETON.store(false, Relaxed);
    }
}
