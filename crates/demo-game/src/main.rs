use std::panic;

use crucible::{
    base::{
        log::{LogLevel, log_str},
        run_loop::{MainLoopEvent, next_event, set_main_loop},
    },
    gfx::{color::Color8, texture::GpuTexture},
};

fn main() {
    panic::set_hook(Box::new(|info| {
        log_str(LogLevel::Fatal, &format!("{info}"));
    }));

    log_str(LogLevel::Info, "Hello, world!");

    set_main_loop(async move {
        loop {
            match next_event().await {
                MainLoopEvent::Redraw => {
                    log_str(LogLevel::Info, "Render!");

                    let mut swapchain = GpuTexture::swapchain();
                    swapchain.clear(Color8::BEIGE);
                }
            }
        }
    });
}
