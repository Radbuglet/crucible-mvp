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
    window::app::{Window, WindowEvent},
};
use glam::{UVec2, Vec2, Vec3};

fn main() {
    setup_logger();
    spawn_task(main_loop());
}

async fn main_loop() {
    let mut window = Window::acquire();
    let mut timer = IntervalTimer::new(1. / 60.);

    let mut my_texture = CpuTexture::new(UVec2::new(100, 100));
    let my_texture_size = my_texture.size();
    for (pos, color) in my_texture.pixels_enumerate_mut() {
        let dist = pos.manhattan_distance(my_texture_size / 2) as f32 / 100.0;
        let dist = 1. - dist;

        *color = Vec3::splat(dist).into();
    }

    let my_texture = my_texture.make_gpu();

    loop {
        futures::select! {
            (times_ticked, _alpha) = timer.next().fuse() => {
                for _ in 0..times_ticked.get() {
                    tracing::info!("Ticking!");
                }

                window.request_redraw();
            }
            ev = window.next_event().fuse() => {
                match ev {
                    WindowEvent::Redraw(mut fb) => {
                        fb.clear(Bgra8::WHITE);

                        fb.draw(
                            GpuDrawArgs::new()
                                .textured(&my_texture)
                                .scale(Vec2::splat(500.))
                                .translate(Vec2::new(100., 200.)),
                        );
                    }
                    WindowEvent::KeyEvent(ev) => {
                        dbg!(ev);
                    }
                    _ => {}
                }
            }
        }
    }
}
