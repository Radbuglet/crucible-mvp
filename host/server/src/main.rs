use arid::World;
use crucible_host_shared::guest::background::BackgroundTasks;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;

use crate::app::App;

mod app;
mod worker;

fn main() -> anyhow::Result<()> {
    // Setup logger
    tracing_subscriber::fmt::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .init();

    // Start main loop
    let background = BackgroundTasks::new();
    let mut background_exec = background.clone().executor(app::main_task(background));

    let mut app = App {
        world: World::new(),
    };

    smol::block_on(background_exec.future(&(), &mut app))
}
