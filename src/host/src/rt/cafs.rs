use std::{
    fs::{self, File},
    io::{self, ErrorKind, Seek},
    ops::DerefMut,
    path::{Path, PathBuf},
};

use anyhow::Context;

use crate::util::file::use_path;

use blake3::Hash;

#[derive(Debug, Clone)]
pub struct Cafs {
    blob_tree: sled::Tree,
    big_blob_path: PathBuf,
}

impl Cafs {
    pub fn new(mut path: PathBuf) -> anyhow::Result<Self> {
        // Ensure that the base directory is present.
        fs::create_dir_all(&path)?;

        // Open the database
        let db = sled::open(&*use_path(&mut path, &[Path::new("cafs.sled")]))?;

        // Construct the big blob path
        let big_blob_path = use_path(&mut path, &[Path::new("v0_cafs_big_blobs")]).clone();

        Ok(Cafs {
            blob_tree: db.open_tree(b"v0_blobs")?,
            big_blob_path,
        })
    }

    pub fn insert_blob(&mut self, hash: Hash, data: &[u8]) -> anyhow::Result<()> {
        self.blob_tree
            .insert(hash.as_bytes(), data)
            .context("failed to insert resource into database")
            .map(|_| ())
    }

    pub fn lookup_blob(&mut self, hash: Hash) -> anyhow::Result<Option<sled::IVec>> {
        self.blob_tree
            .get(hash.as_bytes())
            .context("failed to lookup entry in resource database")
    }

    pub fn insert_big_blob(&mut self, hash: Hash, data: &[u8]) -> anyhow::Result<()> {
        let res_path = use_big_blob_path(&mut self.big_blob_path, hash);

        if let Some(parent) = res_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&*res_path, data)?;

        Ok(())
    }

    pub fn lookup_big_blob(&mut self, hash: Hash) -> anyhow::Result<Option<File>> {
        let res_path = use_big_blob_path(&mut self.big_blob_path, hash);

        // Open file
        let mut file = match File::open(&*res_path) {
            Ok(file) => file,
            Err(err) => {
                if err.kind() == ErrorKind::NotFound {
                    return Ok(None);
                } else {
                    return Err(err.into());
                }
            }
        };

        // Hash the file to ensure that it is valid.
        let actual_hash = {
            let mut hasher = blake3::Hasher::new();
            io::copy(&mut file, &mut hasher).context("failed to take hash of blob")?;
            hasher.finalize()
        };
        file.rewind()?;

        if actual_hash != hash {
            drop(file);
            log::warn!("hash of big blob was incorrect: expected {hash}, got {actual_hash}.");
            if let Err(err) = std::fs::remove_file(&*res_path) {
                log::error!("failed to remove blob filed with hash {hash} but with invalid hash {actual_hash}: {err}");
            }
            return Ok(None);
        }

        Ok(Some(file))
    }
}

fn use_big_blob_path(path: &mut PathBuf, hash: Hash) -> impl DerefMut<Target = &mut PathBuf> {
    let hash_str = hash.to_hex();
    let comps = [Path::new(&hash_str[0..2]), Path::new(&hash_str[2..])];

    use_path(path, &comps)
}
