use std::panic;

use crucible::{
    base::{
        env::current_time,
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
use glam::{DVec2, UVec2, Vec2};

fn main() {
    panic::set_hook(Box::new(|info| {
        log_str(LogLevel::Fatal, &format!("{info}"));
    }));

    spawn_task(main_loop());
}

async fn main_loop() {
    log_str(LogLevel::Info, "Hello, world!");

    let izutsumi = {
        let image = image::load_from_memory_with_format(
            include_bytes!("demo1.png"),
            image::ImageFormat::Png,
        )
        .unwrap();

        let mut image = image.to_rgba8();

        for pixel in image.pixels_mut() {
            let [r, g, b, a] = pixel.0;
            *pixel = image::Rgba([b, g, r, a]);
        }

        CpuTexture::from_raw(
            UVec2::new(image.width(), image.height()),
            bytemuck::cast_vec(image.into_vec()),
        )
        .make_gpu()
    };

    let mut draw_pos = DVec2::ZERO;

    request_loop_wakeup(0.0);

    loop {
        match next_event().await {
            MainLoopEvent::ExitRequested => break,
            MainLoopEvent::TimerExpired => {
                request_loop_wakeup(current_time() + 0.1);

                log_str(LogLevel::Info, "Some time has passed!");
            }
            MainLoopEvent::Client(ClientEvent::Redraw) => {
                log_str(LogLevel::Info, "Render!");

                let mut swapchain = GpuTexture::swapchain();
                swapchain.clear(Color8::BEIGE);

                swapchain.draw(
                    GpuDrawArgs::new()
                        .textured(&izutsumi)
                        .scale(Vec2::splat(500.0))
                        .translate(draw_pos.as_vec2())
                        .tint(Color8::GRAY),
                );

                log_str(LogLevel::Info, &format!("{swapchain:?}"));
            }
            MainLoopEvent::Client(ClientEvent::MouseMoved(pos)) => {
                log_str(LogLevel::Info, &format!("{pos:?}"));
                draw_pos = pos;
                request_redraw();
            }
        }
    }

    confirm_app_exit();
}
