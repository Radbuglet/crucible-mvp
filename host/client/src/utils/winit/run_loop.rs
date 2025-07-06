use std::panic::{self, AssertUnwindSafe};

use winit::{
    application::ApplicationHandler,
    event::{DeviceEvent, DeviceId, StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    window::WindowId,
};

pub fn run_app_fallible<T: 'static>(
    event_loop: EventLoop<T>,
    handler: &mut impl FallibleApplicationHandler<T>,
) -> anyhow::Result<()> {
    struct Wrapper<'a, H> {
        handler: &'a mut H,
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

    impl<T, H> ApplicationHandler<T> for Wrapper<'_, H>
    where
        T: 'static,
        H: FallibleApplicationHandler<T>,
    {
        fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
            self.exec_scoped(event_loop, |this| {
                this.handler.new_events(event_loop, cause)
            });
        }

        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            self.exec_scoped(event_loop, |this| this.handler.resumed(event_loop));
        }

        fn user_event(&mut self, event_loop: &ActiveEventLoop, event: T) {
            self.exec_scoped(event_loop, |this| {
                this.handler.user_event(event_loop, event)
            });
        }

        fn window_event(
            &mut self,
            event_loop: &ActiveEventLoop,
            window_id: WindowId,
            event: WindowEvent,
        ) {
            self.exec_scoped(event_loop, |this| {
                this.handler.window_event(event_loop, window_id, event)
            });
        }

        fn device_event(
            &mut self,
            event_loop: &ActiveEventLoop,
            device_id: DeviceId,
            event: DeviceEvent,
        ) {
            self.exec_scoped(event_loop, |this| {
                this.handler.device_event(event_loop, device_id, event)
            });
        }

        fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
            self.exec_scoped(event_loop, |this| this.handler.about_to_wait(event_loop));
        }

        fn suspended(&mut self, event_loop: &ActiveEventLoop) {
            self.exec_scoped(event_loop, |this| this.handler.suspended(event_loop));
        }

        fn exiting(&mut self, event_loop: &ActiveEventLoop) {
            self.exec_scoped(event_loop, |this| this.handler.exiting(event_loop));
        }

        fn memory_warning(&mut self, event_loop: &ActiveEventLoop) {
            self.exec_scoped(event_loop, |this| this.handler.memory_warning(event_loop));
        }
    }

    let mut app = Wrapper {
        handler,
        error: None,
    };

    event_loop.run_app(&mut app)?;

    if let Some(err) = app.error.take() {
        return Err(err);
    }

    Ok(())
}

pub trait FallibleApplicationHandler<T: 'static = ()> {
    fn new_events(
        &mut self,
        event_loop: &ActiveEventLoop,
        cause: StartCause,
    ) -> anyhow::Result<()> {
        let _ = (event_loop, cause);

        Ok(())
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) -> anyhow::Result<()>;

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: T) -> anyhow::Result<()> {
        let _ = (event_loop, event);

        Ok(())
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) -> anyhow::Result<()>;

    fn device_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        device_id: DeviceId,
        event: DeviceEvent,
    ) -> anyhow::Result<()> {
        let _ = (event_loop, device_id, event);

        Ok(())
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) -> anyhow::Result<()> {
        let _ = event_loop;

        Ok(())
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) -> anyhow::Result<()> {
        let _ = event_loop;

        Ok(())
    }

    fn exiting(&mut self, event_loop: &ActiveEventLoop) -> anyhow::Result<()> {
        let _ = event_loop;

        Ok(())
    }

    fn memory_warning(&mut self, event_loop: &ActiveEventLoop) -> anyhow::Result<()> {
        let _ = event_loop;

        Ok(())
    }
}
