use crucible_host::{
    rt::cafs::Cafs,
    util::{file::read_vec, sha::Sha256Hash},
};

fn main() -> anyhow::Result<()> {
    let mut root = std::env::current_dir()?;
    root.push("demo_fs");

    let mut cafs = Cafs::new(root)?;

    let data = [42, 62];
    let hash = Sha256Hash::digest(&data);
    // cafs.insert_big_blob(hash, &data)?;
    dbg!(read_vec(&mut cafs.lookup_big_blob(hash)?.unwrap())?);

    Ok(())
}
