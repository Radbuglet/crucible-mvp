use std::{
    cell::Cell,
    future,
    pin::Pin,
    task::{self, Waker},
};

// === FFI === //

#[unsafe(no_mangle)]
extern "C" fn crucible_dispatch_redraw() {
    dispatch_event(MainLoopEvent::Redraw);
}

#[unsafe(no_mangle)]
extern "C" fn crucible_dispatch_request_exit() {
    dispatch_event(MainLoopEvent::ExitRequested);
}

// === MainLoop Events === //

#[derive(Debug)]
pub enum MainLoopEvent {
    Redraw,
    ExitRequested,
    Client(ClientEvent),
}

#[derive(Debug)]
pub enum ClientEvent {}

// === MainLoop Executor === //

thread_local! {
    static IS_DISPATCHING: Cell<bool> = const { Cell::new(false) };
    static MAIN_LOOP: Cell<Option<Pin<Box<dyn 'static + Future<Output = ()>>>>> =
        const { Cell::new(None) };

    static DISPATCHED_EVENT: Cell<Option<MainLoopEvent>> = const { Cell::new(None) };
}

pub fn set_main_loop(f: impl 'static + Future<Output = ()>) {
    MAIN_LOOP.set(Some(Box::pin(f)));
}

pub async fn next_event() -> MainLoopEvent {
    future::poll_fn(|_cx| {
        if let Some(ev) = DISPATCHED_EVENT.take() {
            task::Poll::Ready(ev)
        } else {
            task::Poll::Pending
        }
    })
    .await
}

fn dispatch_event(ev: MainLoopEvent) {
    // Detect reentrancy
    assert!(!IS_DISPATCHING.get());
    IS_DISPATCHING.set(true);

    let _reentrancy_guard = scopeguard::guard((), |()| {
        IS_DISPATCHING.set(false);
    });

    // Acquire main loop
    let mut main_loop_guard = scopeguard::guard(MAIN_LOOP.take(), |old_loop| {
        // TODO: Don't blindly override
        MAIN_LOOP.set(old_loop);
    });

    let Some(main_loop) = &mut *main_loop_guard else {
        return;
    };

    // Advance the main loop future until the event is taken.
    DISPATCHED_EVENT.set(Some(ev));

    loop {
        let event_consumed = {
            let ev = DISPATCHED_EVENT.take();
            let event_consumed = ev.is_none();
            DISPATCHED_EVENT.set(ev);
            event_consumed
        };

        if event_consumed {
            break;
        }

        match main_loop
            .as_mut()
            .poll(&mut task::Context::from_waker(Waker::noop()))
        {
            task::Poll::Ready(()) => {
                DISPATCHED_EVENT.set(None);
                drop(scopeguard::ScopeGuard::into_inner(main_loop_guard));
                confirm_app_exit();
                break;
            }
            task::Poll::Pending => {}
        }
    }
}

pub fn confirm_app_exit() {
    #[link(wasm_import_module = "crucible")]
    unsafe extern "C" {
        fn confirm_app_exit();
    }

    unsafe { confirm_app_exit() };
}
