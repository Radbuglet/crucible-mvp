use std::{
    future::Future,
    io::Write,
    path::{Path, PathBuf},
    pin::Pin,
    task,
};

use anyhow::Context;

use async_tempfile::TempFile;
use blake3::{Hash, Hasher};

use tokio::{
    fs::{self, File},
    io::{self, AsyncWrite, ErrorKind},
    sync::OnceCell,
};

use crate::util::{io::SyncWriteAsAsync, map::FxArcMap};

#[derive(Debug)]
pub struct Cafs {
    blob_tree: sled::Tree,
    big_blob_path: PathBuf,
    blob_load_tasks: FxArcMap<Hash, OnceCell<()>>,
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
            blob_load_tasks: FxArcMap::new(),
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

    fn blob_path(&self, hash: &Hash) -> PathBuf {
        let hash = hash.to_hex();
        let mut blob_path = self.big_blob_path.clone();
        blob_path.push(&hash[0..2]);
        blob_path.push(&hash[2..]);
        blob_path
    }

    pub async fn load_big_blob<F>(&self, hash: Hash, load: F) -> anyhow::Result<File>
    where
        F: FnOnce(BigBlobWriter<'_>) -> Box<dyn Future<Output = anyhow::Result<()>> + '_>,
    {
        let blob_load_task = self.blob_load_tasks.get(&hash, || (hash, OnceCell::new()));

        blob_load_task
            .get_or_try_init::<anyhow::Error, _, _>(|| async {
                // Determine the path of the blob
                let blob_path = self.blob_path(&hash);

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
                            return Ok(());
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
				let mut file = TempFile::new().await?;

				// ...and allow the user to write to it.
				let mut hasher = Hasher::new();
				Box::into_pin(load(BigBlobWriter { file: Pin::new(&mut file), hasher: &mut hasher })).await?;
				let actual_hash = hasher.finalize();

				// Validate the hash to ensure that the blob was downloaded correctly.
				anyhow::ensure!(
					actual_hash == hash,
					"mismatched download file hash; expected {hash}, got {actual_hash}",
				);

				// Move it into the main blob directory and give it back. We allow ourselves to
				// overwrite the previous file because the previous file may be an old corrupted one
				// that failed the integrity check.
				fs::rename(file.file_path(), blob_path).await?;
				std::mem::forget(file);

                Ok(())
            })
            .await?;

        Ok(File::open(self.blob_path(&hash)).await?)
    }
}

#[derive(Debug)]
pub struct BigBlobWriter<'a> {
    file: Pin<&'a mut File>,
    hasher: &'a mut Hasher,
}

impl Unpin for BigBlobWriter<'_> {}

impl AsyncWrite for BigBlobWriter<'_> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut task::Context<'_>,
        buf: &[u8],
    ) -> task::Poll<Result<usize, io::Error>> {
        let me = self.get_mut();
        let res = me.file.as_mut().poll_write(cx, buf);
        if let task::Poll::Ready(Ok(size)) = &res {
            let _ = me.hasher.write(&buf[0..*size]);
        }
        res
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut task::Context<'_>,
    ) -> task::Poll<Result<(), io::Error>> {
        self.get_mut().file.as_mut().poll_flush(cx)
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut task::Context<'_>,
    ) -> task::Poll<Result<(), io::Error>> {
        self.get_mut().file.as_mut().poll_shutdown(cx)
    }
}
