use std::{env, fs, sync::Arc};

use anyhow::Context;
use arid::{Strong, World};
use arid_entity::EntityHandle;
use futures::executor::block_on;
use wasmlink_wasmtime::{WslLinker, WslStore, WslStoreExt, WslStoreState};
use winit::{
    event::{KeyEvent, StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{WindowAttributes, WindowId},
};

use crate::{
    bindings::{env::EnvBindingsHandle, gfx::GfxBindingsHandle},
    services::window::{WindowManagerHandle, create_gfx_context},
    utils::winit::{FallibleApplicationHandler, run_app_fallible},
};

#[derive(Debug)]
struct App {
    world: World,
    root: Strong<EntityHandle>,
    engine: wasmtime::Engine,
    module: wasmtime::Module,
    init: Option<AppInitState>,
}

#[derive(Debug)]
struct AppInitState {
    window_mgr: WindowManagerHandle,
    env_bindings: EnvBindingsHandle,
    gfx_bindings: GfxBindingsHandle,
    store: WslStore,
    instance: wasmtime::Instance,
}

impl FallibleApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) -> anyhow::Result<()> {
        let w = &mut self.world;

        if self.init.is_some() {
            return Ok(());
        }

        block_on(async {
            // Setup graphics
            let window = Arc::new(
                event_loop.create_window(
                    WindowAttributes::default()
                        .with_title("Crucible")
                        .with_visible(false),
                )?,
            );

            let (gfx, surface) = create_gfx_context(window.clone()).await?;
            let window_mgr = WindowManagerHandle::new(gfx, w);
            self.root.add(window_mgr.clone(), w);

            window_mgr.create_window(window.clone(), surface, w);

            // Setup WASM linker
            let mut linker = WslLinker::new(&self.engine);

            let env_bindings = self.root.add(EnvBindingsHandle::new(w), w);
            env_bindings.install(&mut linker)?;

            let gfx_bindings = self
                .root
                .add(GfxBindingsHandle::new(window_mgr.as_weak(), w), w);

            gfx_bindings.install(&mut linker)?;

            linker.define_unknown_imports_as_traps(&self.module)?;

            // Instantiate module
            let mut store = wasmtime::Store::new(&self.engine, WslStoreState::default());

            let instance = linker.instantiate(&mut store, &self.module)?;

            store.setup_wsl_exports(instance)?;

            store.run_root(w, |cx| -> anyhow::Result<()> {
                instance
                    .get_typed_func::<(u32, u32), u32>(cx.cx_mut(), "main")?
                    .call(cx.cx_mut(), (0, 0))?;

                Ok(())
            })?;

            // Mark as initialized
            window.set_visible(true);

            self.init = Some(AppInitState {
                window_mgr: window_mgr.as_weak(),
                env_bindings,
                gfx_bindings,
                store,
                instance,
            });

            Ok(())
        })
    }

    fn new_events(
        &mut self,
        _event_loop: &ActiveEventLoop,
        cause: StartCause,
    ) -> anyhow::Result<()> {
        let Some(init) = &mut self.init else {
            return Ok(());
        };

        if matches!(cause, StartCause::ResumeTimeReached { .. }) {
            init.store
                .run_root(&mut self.world, |cx| init.env_bindings.poll_timeouts(cx))?;
        }

        Ok(())
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) -> anyhow::Result<()> {
        let Some(init) = &mut self.init else {
            return Ok(());
        };

        match &event {
            WindowEvent::RedrawRequested => {
                // TODO
            }
            WindowEvent::CursorMoved { position, .. } => {
                // TODO
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key,
                        logical_key,
                        text,
                        location,
                        state,
                        repeat,
                        ..
                    },
                ..
            } => {
                // TODO
            }
            WindowEvent::CloseRequested => {
                // TODO
            }
            _ => {}
        }

        Ok(())
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) -> anyhow::Result<()> {
        let Some(init) = &mut self.init else {
            return Ok(());
        };

        // TODO

        if let Some(timeout) = init.env_bindings.earliest_timeout(&self.world) {
            event_loop.set_control_flow(ControlFlow::WaitUntil(timeout));
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }

        self.world.flush();

        Ok(())
    }
}

pub fn run_app() -> anyhow::Result<()> {
    // Creating windowing services
    tracing::info!("Setting up windowing and graphics contexts.");

    let mut world = World::new();
    let event_loop = EventLoop::new()?;

    let root = EntityHandle::new(&mut world);
    root.with_label("root", &mut world);

    // Setup WASM runtime
    tracing::info!("Setting up WASM runtime.");
    let engine = wasmtime::Engine::new(&wasmtime::Config::default())?;

    // Load module
    tracing::info!("Loading module.");

    let module_path = env::args().nth(1).context("no module supplied")?;
    let module = fs::read(&module_path)
        .with_context(|| format!("failed to read module at `{module_path}`"))?;

    let module = wasmtime::Module::new(&engine, module)?;

    // Start main loop
    tracing::info!("Starting main loop!");

    run_app_fallible(
        event_loop,
        &mut App {
            world,
            root,
            engine,
            module,
            init: None,
        },
    )?;

    Ok(())
}
