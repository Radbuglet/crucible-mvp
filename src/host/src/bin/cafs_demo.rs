use crucible_host::rt::cafs::Cafs;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cafs = Cafs::new(&std::env::current_dir()?.join("demo_fs")).await?;

    Ok(())
}
