use anyhow::Context;

use crate::util::sha::Sha256Hash;

#[derive(Debug, Clone)]
pub struct Cafs {
    blob_tree: sled::Tree,
}

impl Cafs {
    pub fn new(db: sled::Db) -> anyhow::Result<Self> {
        Ok(Cafs {
            blob_tree: db.open_tree(b"v0_blobs")?,
        })
    }

    pub fn insert_blob(&mut self, hash: Sha256Hash, data: &[u8]) -> anyhow::Result<()> {
        self.blob_tree
            .insert(hash.0, data)
            .context("failed to insert resource into database")
            .map(|_| ())
    }

    pub fn lookup_blob(&mut self, hash: Sha256Hash) -> anyhow::Result<Option<sled::IVec>> {
        self.blob_tree
            .get(hash.0)
            .context("failed to lookup entry in resource database")
    }
}
