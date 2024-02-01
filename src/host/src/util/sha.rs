use core::fmt;
use std::io;

use sha2::{Digest, Sha256};

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
#[repr(transparent)]
pub struct Sha256Hash(pub [u8; 32]);

impl Sha256Hash {
    pub fn digest(data: &[u8]) -> Self {
        Self(Sha256::digest(data).into())
    }

    pub fn digest_reader(data: &mut impl io::Read) -> io::Result<Self> {
        let mut sha = Sha256::default();
        io::copy(data, &mut sha)?;
        Ok(Self(sha.finalize().into()))
    }
}

impl fmt::Display for Sha256Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.0 {
            write!(f, "{byte:x}")?;
        }
        Ok(())
    }
}
