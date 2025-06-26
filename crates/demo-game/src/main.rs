use std::panic;

use crucible::{
    app::run_loop::{MainLoopEvent, next_event, set_main_loop},
    base::log::{LogLevel, log_str},
    gfx::{
        color::Color8,
        texture::{CpuTexture, GpuDrawArgs, GpuTexture},
    },
};
use glam::{UVec2, Vec2};

fn main() {
    panic::set_hook(Box::new(|info| {
        log_str(LogLevel::Fatal, &format!("{info}"));
    }));

    set_main_loop(main_loop());
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

    loop {
        match next_event().await {
            MainLoopEvent::Redraw => {
                log_str(LogLevel::Info, "Render!");

                let mut swapchain = GpuTexture::swapchain();
                swapchain.clear(Color8::BEIGE);

                swapchain.draw(
                    GpuDrawArgs::new()
                        .textured(&izutsumi)
                        .scale(Vec2::splat(500.0))
                        .tint(Color8::GRAY),
                );

                log_str(LogLevel::Info, &format!("{swapchain:?}"));
            }
            MainLoopEvent::ExitRequested => break,
            MainLoopEvent::Client(_) => todo!(),
        }
    }
}
