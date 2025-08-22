use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SbDownloadReq {
    pub hash: blake3::Hash,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CbDownloadRes {
    serial: u32,
}
