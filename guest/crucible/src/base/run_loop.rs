use std::{cell::RefCell, rc::Rc};

use futures::{
    StreamExt,
    channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded},
};

use crate::window::run_loop::ClientEvent;

use super::task::wake_executor;

// === Event Sink === //

#[derive(Debug)]
#[non_exhaustive]
pub enum MainLoopEvent {
    ExitRequested,
    TimerExpired,
    Client(ClientEvent),
}

thread_local! {
    static EVENTS: (UnboundedSender<MainLoopEvent>, Rc<RefCell<UnboundedReceiver<MainLoopEvent>>>) = {
        let (tx, rx) = unbounded();

        (tx, Rc::new(RefCell::new(rx)))
    };
}

#[expect(clippy::await_holding_refcell_ref)]
pub async fn next_event() -> MainLoopEvent {
    let rx = EVENTS.with(|(_, rx)| rx.clone());

    let mut rx = rx
        .try_borrow_mut()
        .expect("`next_event` can only be called by one task at a time");

    rx.next().await.unwrap()
}

pub(crate) fn dispatch_event(event: MainLoopEvent) {
    EVENTS.with(|(tx, _)| {
        tx.unbounded_send(event)
            .expect("receiver already shut down")
    });

    wake_executor();
}

#[unsafe(no_mangle)]
extern "C" fn crucible_dispatch_request_exit() {
    dispatch_event(MainLoopEvent::ExitRequested);
}

#[unsafe(no_mangle)]
extern "C" fn crucible_dispatch_timer_expired() {
    dispatch_event(MainLoopEvent::TimerExpired);
}

// === Operations === //

pub fn confirm_app_exit() {
    #[link(wasm_import_module = "crucible")]
    unsafe extern "C" {
        fn confirm_app_exit();
    }

    unsafe { confirm_app_exit() };
}

pub fn request_loop_wakeup(at: f64) {
    #[link(wasm_import_module = "crucible")]
    unsafe extern "C" {
        fn request_loop_wakeup(at: f64);
    }

    unsafe { request_loop_wakeup(at) };
}
