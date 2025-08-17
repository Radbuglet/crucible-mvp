use std::{env, fs, sync::Arc};

use anyhow::Context;
use arid::{Strong, World};
use arid_entity::EntityHandle;
use futures::executor::block_on;
use wasmlink_wasmtime::{WslLinker, WslStore, WslStoreExt, WslStoreState};
use winit::{
    event::{KeyEvent, MouseButton, StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard,
    window::{WindowAttributes, WindowId},
};

use crate::{
    bindings::{env::EnvBindingsHandle, gfx::GfxBindingsHandle},
    services::window::{WindowManagerHandle, WindowStateHandle, create_gfx_context},
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
    main_window: WindowStateHandle,
    store: WslStore,
    _instance: wasmtime::Instance,
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

            let main_window = window_mgr.create_window(window.clone(), surface, w);

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

            store.run_wsl_root(w, |cx| -> anyhow::Result<()> {
                instance
                    .get_typed_func::<(u32, u32), u32>(cx.cx_mut(), "main")?
                    .call(cx.cx_mut(), (0, 0))?;

                Ok(())
            })?;

            if gfx_bindings.callbacks(w).is_none() {
                anyhow::bail!("`Window` must be `acquire`'d before the first `.await`-point");
            }

            // Mark as initialized
            window.set_visible(true);

            self.init = Some(AppInitState {
                window_mgr: window_mgr.as_weak(),
                env_bindings,
                gfx_bindings,
                main_window,
                store,
                _instance: instance,
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
                .run_wsl_root(&mut self.world, |cx| init.env_bindings.poll_timeouts(cx))?;
        }

        Ok(())
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) -> anyhow::Result<()> {
        let w = &mut self.world;

        let Some(init) = &mut self.init else {
            return Ok(());
        };

        match &event {
            WindowEvent::RedrawRequested => {
                let Some(cbs) = init.gfx_bindings.callbacks(w) else {
                    return Ok(());
                };

                let window = init.window_mgr.lookup(window_id, w);

                window.redraw(
                    |fb, w| -> anyhow::Result<()> {
                        let handle = init.gfx_bindings.create_texture(fb.texture.clone(), w)?;

                        init.store.run_wsl_root(w, |cx| {
                            cbs.redraw_requested.call(
                                cx,
                                &crucible_abi::RedrawRequestedArgs {
                                    fb: crucible_abi::GpuTextureHandle { raw: handle },
                                    size: crucible_abi::UVec2 {
                                        x: fb.texture.width(),
                                        y: fb.texture.height(),
                                    },
                                },
                            )
                        })?;

                        Ok(())
                    },
                    w,
                )?;
            }
            WindowEvent::CursorMoved { position, .. } => {
                let Some(cbs) = init.gfx_bindings.callbacks(w) else {
                    return Ok(());
                };

                init.store.run_wsl_root(w, |cx| {
                    cbs.mouse_moved.call(
                        cx,
                        &crucible_abi::DVec2 {
                            x: position.x,
                            y: position.y,
                        },
                    )
                })?;
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let Some(cbs) = init.gfx_bindings.callbacks(w) else {
                    return Ok(());
                };

                init.store.run_wsl_root(w, |cx| {
                    cbs.mouse_event.call(
                        cx,
                        &crucible_abi::MouseEvent {
                            button: match button {
                                MouseButton::Left => crucible_abi::MouseButton::Left(()),
                                MouseButton::Right => crucible_abi::MouseButton::Right(()),
                                MouseButton::Middle => crucible_abi::MouseButton::Middle(()),
                                MouseButton::Back => crucible_abi::MouseButton::Back(()),
                                MouseButton::Forward => crucible_abi::MouseButton::Forward(()),
                                MouseButton::Other(id) => crucible_abi::MouseButton::Other(*id),
                            },
                            pressed: state.is_pressed(),
                        },
                    )
                })?;
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
                let Some(cbs) = init.gfx_bindings.callbacks(w) else {
                    return Ok(());
                };

                init.store.run_wsl_root(w, |cx| {
                    cbs.key_event.call(
                        cx,
                        &crucible_abi::KeyEvent {
                            physical_key: match physical_key {
                                keyboard::PhysicalKey::Code(code) => Some(*code as u32),
                                keyboard::PhysicalKey::Unidentified(_) => None,
                            },
                            logical_key: match logical_key {
                                keyboard::Key::Named(named_key) => {
                                    crucible_abi::LogicalKey::Named(*named_key as u32)
                                }
                                keyboard::Key::Character(ch) => {
                                    crucible_abi::LogicalKey::Character(ch.as_str())
                                }
                                keyboard::Key::Unidentified(code) => {
                                    crucible_abi::LogicalKey::Unidentified(match code {
                                        keyboard::NativeKey::Unidentified => {
                                            crucible_abi::NativeKey::Unidentified(())
                                        }
                                        keyboard::NativeKey::Android(v) => {
                                            crucible_abi::NativeKey::Android(*v)
                                        }
                                        keyboard::NativeKey::MacOS(v) => {
                                            crucible_abi::NativeKey::MacOS(*v)
                                        }
                                        keyboard::NativeKey::Windows(v) => {
                                            crucible_abi::NativeKey::Windows(*v)
                                        }
                                        keyboard::NativeKey::Xkb(v) => {
                                            crucible_abi::NativeKey::Xkb(*v)
                                        }
                                        keyboard::NativeKey::Web(v) => {
                                            crucible_abi::NativeKey::Web(v.as_str())
                                        }
                                    })
                                }
                                keyboard::Key::Dead(ch) => crucible_abi::LogicalKey::Dead(*ch),
                            },
                            text: text.as_deref(),
                            location: *location as u32,
                            pressed: state.is_pressed(),
                            repeat: *repeat,
                        },
                    )
                })?;
            }
            WindowEvent::CloseRequested => {
                let Some(cbs) = init.gfx_bindings.callbacks(w) else {
                    return Ok(());
                };

                init.store
                    .run_wsl_root(w, |cx| cbs.exit_requested.call(cx, &()))?;
            }
            _ => {}
        }

        Ok(())
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) -> anyhow::Result<()> {
        let w = &mut self.world;

        let Some(init) = &mut self.init else {
            return Ok(());
        };

        if init.gfx_bindings.take_redraw_request(w) && !init.main_window.is_in_live_resize(w) {
            init.main_window.window(w).request_redraw();
        }

        if let Some(timeout) = init.env_bindings.earliest_timeout(w) {
            event_loop.set_control_flow(ControlFlow::WaitUntil(timeout));
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }

        if init.gfx_bindings.callbacks(w).is_none() {
            event_loop.exit();
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
