use sha2::{Digest, Sha256};

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
#[repr(transparent)]
pub struct Sha256Hash(pub [u8; 32]);

impl Sha256Hash {
    pub fn digest(data: &[u8]) -> Self {
        Self(Sha256::digest(data).into())
    }
}
