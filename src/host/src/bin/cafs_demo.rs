use blake3::Hasher;
use crucible_host::{rt::cafs::Cafs, util::io::SyncWriteAsAsync};
use tokio::{fs::File, io};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cafs = Cafs::new(&std::env::current_dir()?.join("demo_fs")).await?;

    let path = "big.zip";
    let hash = {
        let mut in_file = File::open(path).await?;
        let mut hasher = Hasher::new();
        io::copy(&mut in_file, &mut SyncWriteAsAsync(&mut hasher)).await?;
        hasher.finalize()
    };

    cafs.load_big_blob(hash, |mut writer| {
        Box::new(async move {
            io::copy(&mut File::open(path).await?, &mut writer).await?;
            Ok(())
        })
    })
    .await?;

    Ok(())
}
