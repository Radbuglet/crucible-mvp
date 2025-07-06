use std::panic;

use crucible::{
    base::{
        log::{LogLevel, log_str},
        run_loop::{MainLoopEvent, confirm_app_exit, next_event, request_loop_wakeup},
        task::spawn_task,
    },
    gfx::{
        color::Color8,
        texture::{CpuTexture, GpuDrawArgs, GpuTexture},
    },
    window::run_loop::{ClientEvent, request_redraw},
};
use cruise::{
    arid::World,
    base::run_loop::{StepResult, StepTimer},
};
use glam::{UVec2, Vec2, Vec3};

fn main() {
    panic::set_hook(Box::new(|info| {
        log_str(LogLevel::Fatal, &format!("{info}"));
    }));

    spawn_task(main_loop());
}

async fn main_loop() {
    let mut w = World::new();
    let w = &mut w;

    let mut timer = StepTimer::new(1. / 60.);

    let mut my_texture = CpuTexture::new(UVec2::new(100, 100));
    let my_texture_size = my_texture.size();
    for (pos, color) in my_texture.pixels_enumerate_mut() {
        let dist = pos.manhattan_distance(my_texture_size / 2) as f32 / 100.0;
        let dist = 1. - dist;

        *color = Vec3::splat(dist).into();
    }

    let my_texture = my_texture.make_gpu();

    request_loop_wakeup(0.0);

    loop {
        match next_event().await {
            MainLoopEvent::ExitRequested => {
                break;
            }
            MainLoopEvent::TimerExpired => {
                let StepResult {
                    times_ticked,
                    wait_until,
                } = timer.tick();

                for _ in 0..times_ticked {
                    // TODO
                }

                request_loop_wakeup(wait_until);
                request_redraw();
            }
            MainLoopEvent::Client(ClientEvent::Redraw) => {
                let mut fb = GpuTexture::swapchain();

                fb.clear(Color8::WHITE);

                fb.draw(
                    GpuDrawArgs::new()
                        .textured(&my_texture)
                        .scale(Vec2::splat(500.))
                        .translate(Vec2::new(100., 200.)),
                );
            }
            _ => {}
        }
    }

    confirm_app_exit();
}
