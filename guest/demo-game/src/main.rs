use std::collections::HashSet;

use crucible::{
    base::{
        env::IntervalTimer,
        logging::{setup_logger, tracing},
        task::{
            futures::{self, FutureExt},
            spawn_task,
        },
    },
    gfx::{
        color::Bgra8,
        texture::{CpuTexture, GpuDrawArgs},
    },
    shell::socket::LoginSocket,
    window::{
        app::{Window, WindowEvent},
        defs::{KeyCode, PhysicalKey},
    },
};
use glam::Vec2;

fn main() {
    setup_logger();
    spawn_task(main_loop());
}

async fn main_loop() {
    let mut window = Window::acquire();
    let mut timer = IntervalTimer::new(1. / 60.);

    let socket = LoginSocket::connect("127.0.0.1:8080").await.unwrap();
    let info = socket.info().await.unwrap();

    tracing::info!("{:#?}", info);

    _ = socket.play(info.content_hash).await.unwrap().unwrap();

    let my_texture = CpuTexture::from_rgba8(
        image::load_from_memory(include_bytes!("demo1.png"))
            .unwrap()
            .to_rgba8(),
    )
    .make_gpu();

    let mut pos = Vec2::ZERO;
    let mut keys_down = HashSet::<KeyCode>::default();

    loop {
        futures::select! {
            (times_ticked, _alpha) = timer.next().fuse() => {
                for _ in 0..times_ticked.get() {
                    let mut heading = Vec2::ZERO;

                    if keys_down.contains(&KeyCode::KeyA) {
                        heading += Vec2::NEG_X;
                    }

                    if keys_down.contains(&KeyCode::KeyD) {
                        heading += Vec2::X;
                    }

                    if keys_down.contains(&KeyCode::KeyW) {
                        heading += Vec2::NEG_Y;
                    }

                    if keys_down.contains(&KeyCode::KeyS) {
                        heading += Vec2::Y;
                    }

                    pos += heading.normalize_or_zero() * timer.interval() as f32 * 500.;
                }

                window.request_redraw();
            }
            ev = window.next_event().fuse() => {
                match ev {
                    WindowEvent::Redraw(mut fb) => {
                        tracing::info!("Ping: {:?}s", socket.rtt());
                        fb.clear(Bgra8::RED);

                        fb.draw(
                            GpuDrawArgs::new()
                                .textured(&my_texture)
                                .scale(Vec2::splat(50.))
                                .translate(pos),
                        );
                    }
                    WindowEvent::KeyEvent(ev) => {
                        let PhysicalKey::KeyCode(key) = ev.physical_key else {
                            continue;
                        };

                        if ev.pressed {
                            keys_down.insert(key);
                        }    else {
                            keys_down.remove(&key);
                        }
                    }
                    WindowEvent::ExitRequested => {
                        break;
                    }
                    _ => {}
                }
            }
        }
    }
}
