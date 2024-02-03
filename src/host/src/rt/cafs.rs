use std::path::{Path, PathBuf};

use anyhow::Context;

use blake3::{Hash, Hasher};

use tokio::{
    fs::{self, File},
    io::{self, AsyncSeekExt, ErrorKind},
    sync::OnceCell,
};

use crate::util::{
    io::SyncWriteAsAsync,
    map::{FxArcMap, FxArcMapRef},
};

#[derive(Debug)]
pub struct Cafs {
    blob_tree: sled::Tree,
    big_blob_path: PathBuf,
    big_blob_fds: FxArcMap<Hash, OnceCell<File>>,
}

impl Cafs {
    pub async fn new(path: &Path) -> anyhow::Result<Self> {
        // Ensure that the base directory is present.
        fs::create_dir_all(path).await?;

        // Open the database
        let db = sled::open(path.join("cafs.sled"))?;

        // Construct the big blob path
        let big_blob_path = path.join("v0_cafs_big_blobs");

        Ok(Cafs {
            blob_tree: db.open_tree(b"v0_blobs")?,
            big_blob_path,
            big_blob_fds: FxArcMap::new(),
        })
    }

    pub fn insert_blob(&self, hash: Hash, data: &[u8]) -> anyhow::Result<()> {
        self.blob_tree
            .insert(hash.as_bytes(), data)
            .context("failed to insert resource into database")
            .map(|_| ())
    }

    pub fn lookup_blob(&self, hash: Hash) -> anyhow::Result<Option<sled::IVec>> {
        self.blob_tree
            .get(hash.as_bytes())
            .context("failed to lookup entry in resource database")
    }

    pub async fn load_big_blob(&self, hash: Hash) -> anyhow::Result<CafsBlob> {
        let entry = self.big_blob_fds.get(&hash, || (hash, OnceCell::new()));

        entry
            .get_or_try_init::<anyhow::Error, _, _>(|| async {
                // Determine the path of the blob
                let blob_path = {
                    let hash = hash.to_hex();
                    let mut blob_path = self.big_blob_path.clone();
                    blob_path.push(&hash[0..2]);
                    blob_path.push(&hash[2..]);
                    blob_path
                };

                // Ensure that its parent directory is present
                if let Some(parent) = blob_path.parent() {
                    fs::create_dir_all(parent).await?;
                }

                // Attempt to load the file as it is on disk
                match File::open(&blob_path).await {
                    Ok(mut file) => {
                        // Validate the file's integrity
                        let mut hasher = Hasher::new();
                        io::copy(&mut file, &mut SyncWriteAsAsync(&mut hasher)).await?;
                        let actual_hash = hasher.finalize();

                        if hash == actual_hash {
                            file.rewind().await?;
                            return Ok(file);
                        }

						// If the file doesn't match, we need to re-create it.
						log::warn!("blob with expected hash {hash} doesn't match actual hash {actual_hash}");

						// (fallthrough)
                    }
                    Err(err) => {
						// If the operation failed for some reason other than the file being un-findable,
						// we have an unrecoverable IO error which we should raise.
						if err.kind() != ErrorKind::NotFound {
							return Err(err.into());
						}

						// (fallthrough)
					},
                }

				// Otherwise, create a temporary file.

				// ...and allow the user to write to it.
				// TODO

				// Validate the hash to ensure that the blob was downloaded correctly.
				// TODO

                todo!();
            })
            .await?;

        Ok(CafsBlob(entry))
    }
}

pub struct CafsBlob(FxArcMapRef<Hash, OnceCell<File>>);
