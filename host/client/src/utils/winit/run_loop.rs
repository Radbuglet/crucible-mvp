use std::{
    future,
    panic::{self, AssertUnwindSafe},
    sync::Arc,
    task,
};

use crucible_host_shared::guest::background;
use winit::{
    application::ApplicationHandler,
    event::{DeviceEvent, DeviceId, StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
    window::WindowId,
};

pub type BackgroundTasks<T> = background::BackgroundTasks<ActiveEventLoop, T>;
type BackgroundTasksExecutor<T> = background::BackgroundTaskExecutor<ActiveEventLoop, T, ()>;

pub fn run_winit(event_loop: EventLoop<()>, handler: &mut impl WinitHandler) -> anyhow::Result<()> {
    struct WinitWaker {
        // TODO: Avoid redundant wake-ups.
        proxy: EventLoopProxy<()>,
    }

    impl task::Wake for WinitWaker {
        fn wake(self: Arc<Self>) {
            self.wake_by_ref();
        }

        fn wake_by_ref(self: &Arc<Self>) {
            _ = self.proxy.send_event(());
        }
    }

    struct Wrapper<'a, H: 'static> {
        handler: &'a mut H,
        _waker: Arc<WinitWaker>,
        erased_waker: task::Waker,
        background: BackgroundTasksExecutor<H>,
        error: Option<anyhow::Error>,
    }

    impl<H> Wrapper<'_, H> {
        fn exec_scoped(
            &mut self,
            event_loop: &ActiveEventLoop,
            f: impl FnOnce(&mut Self) -> anyhow::Result<()>,
        ) {
            if self.error.is_some() {
                return;
            }

            match panic::catch_unwind(AssertUnwindSafe(|| f(self))) {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    self.error = Some(e);
                    event_loop.exit();
                }
                Err(_) => {
                    self.error = Some(anyhow::anyhow!("run loop panicked"));
                    event_loop.exit();
                }
            }
        }
    }

    impl<H> ApplicationHandler for Wrapper<'_, H>
    where
        H: WinitHandler,
    {
        fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
            self.exec_scoped(event_loop, |this| {
                this.handler
                    .new_events(event_loop, this.background.handle(), cause)
            });
        }

        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            self.exec_scoped(event_loop, |this| {
                this.handler.resumed(event_loop, this.background.handle())
            });
        }

        fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: ()) {
            // (just used for wakeups)
        }

        fn window_event(
            &mut self,
            event_loop: &ActiveEventLoop,
            window_id: WindowId,
            event: WindowEvent,
        ) {
            self.exec_scoped(event_loop, |this| {
                this.handler
                    .window_event(event_loop, this.background.handle(), window_id, event)
            });
        }

        fn device_event(
            &mut self,
            event_loop: &ActiveEventLoop,
            device_id: DeviceId,
            event: DeviceEvent,
        ) {
            self.exec_scoped(event_loop, |this| {
                this.handler
                    .device_event(event_loop, this.background.handle(), device_id, event)
            });
        }

        fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
            self.exec_scoped(event_loop, |this| {
                this.handler
                    .about_to_wait(event_loop, this.background.handle())?;

                let res = this.background.poll(
                    event_loop,
                    this.handler,
                    &mut task::Context::from_waker(&this.erased_waker),
                );

                match res {
                    task::Poll::Ready(v) => v,
                    task::Poll::Pending => Ok(()),
                }
            });
        }

        fn suspended(&mut self, event_loop: &ActiveEventLoop) {
            self.exec_scoped(event_loop, |this| {
                this.handler.suspended(event_loop, this.background.handle())
            });
        }

        fn exiting(&mut self, event_loop: &ActiveEventLoop) {
            self.exec_scoped(event_loop, |this| {
                this.handler.exiting(event_loop, this.background.handle())
            });
        }

        fn memory_warning(&mut self, event_loop: &ActiveEventLoop) {
            self.exec_scoped(event_loop, |this| {
                this.handler
                    .memory_warning(event_loop, this.background.handle())
            });
        }
    }

    let background = BackgroundTasks::new().executor(future::pending());

    let waker = Arc::new(WinitWaker {
        proxy: event_loop.create_proxy(),
    });
    let erased_waker = task::Waker::from(waker.clone());

    let mut app = Wrapper {
        handler,
        _waker: waker,
        erased_waker,
        background,
        error: None,
    };

    event_loop.run_app(&mut app)?;

    if let Some(err) = app.error.take() {
        return Err(err);
    }

    Ok(())
}

pub trait WinitHandler: Sized + 'static {
    fn new_events(
        &mut self,
        event_loop: &ActiveEventLoop,
        background: &BackgroundTasks<Self>,
        cause: StartCause,
    ) -> anyhow::Result<()> {
        let _ = (event_loop, background, cause);

        Ok(())
    }

    fn resumed(
        &mut self,
        event_loop: &ActiveEventLoop,
        background: &BackgroundTasks<Self>,
    ) -> anyhow::Result<()>;

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        background: &BackgroundTasks<Self>,
        window_id: WindowId,
        event: WindowEvent,
    ) -> anyhow::Result<()>;

    fn device_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        background: &BackgroundTasks<Self>,
        device_id: DeviceId,
        event: DeviceEvent,
    ) -> anyhow::Result<()> {
        let _ = (event_loop, background, device_id, event);

        Ok(())
    }

    fn about_to_wait(
        &mut self,
        event_loop: &ActiveEventLoop,
        background: &BackgroundTasks<Self>,
    ) -> anyhow::Result<()> {
        let _ = (event_loop, background);

        Ok(())
    }

    fn suspended(
        &mut self,
        event_loop: &ActiveEventLoop,
        background: &BackgroundTasks<Self>,
    ) -> anyhow::Result<()> {
        let _ = (event_loop, background);

        Ok(())
    }

    fn exiting(
        &mut self,
        event_loop: &ActiveEventLoop,
        background: &BackgroundTasks<Self>,
    ) -> anyhow::Result<()> {
        let _ = (event_loop, background);

        Ok(())
    }

    fn memory_warning(
        &mut self,
        event_loop: &ActiveEventLoop,
        background: &BackgroundTasks<Self>,
    ) -> anyhow::Result<()> {
        let _ = (event_loop, background);

        Ok(())
    }
}
