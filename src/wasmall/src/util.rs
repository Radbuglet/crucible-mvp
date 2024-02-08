use std::io::ErrorKind;

// === ExtensionFor === //

mod extension_for {
    pub trait Sealed<T: ?Sized> {}
}

pub trait ExtensionFor<T: ?Sized>: extension_for::Sealed<T> {
    fn v(&self) -> &T;

    fn v_mut(&mut self) -> &mut T;
}

impl<T: ?Sized> extension_for::Sealed<T> for T {}

impl<T: ?Sized> ExtensionFor<T> for T {
    fn v(&self) -> &T {
        self
    }

    fn v_mut(&mut self) -> &mut T {
        self
    }
}

// === VecExt === //

pub trait VecExt<T>: ExtensionFor<Vec<T>> {
    fn ensure_length(&mut self, len: usize)
    where
        T: Default,
    {
        if self.v_mut().len() < len {
            self.v_mut().resize_with(len, Default::default);
        }
    }

    fn ensure_index(&mut self, index: usize) -> &mut T
    where
        T: Default,
    {
        self.ensure_length(index + 1);
        &mut self.v_mut()[index]
    }
}

impl<T> VecExt<T> for Vec<T> {}

pub trait SliceExt<T>: ExtensionFor<[T]> {
    fn limit_len(&self, len: usize) -> &[T] {
        &self.v()[..self.v().len().min(len)]
    }

    fn to_array<const N: usize>(&self) -> [T; N]
    where
        T: Copy,
    {
        std::array::from_fn(|i| self.v()[i])
    }
}

impl<T> SliceExt<T> for [T] {}

// === Codec Utils === //

pub trait Leb128ReadExt: ExtensionFor<[u8]> {
    fn read_u8(&self) -> anyhow::Result<u8> {
        anyhow::ensure!(self.v().len() >= 1);
        Ok(self.v()[0])
    }

    fn read_u32(&self) -> anyhow::Result<u32> {
        anyhow::ensure!(self.v().len() >= 4);
        Ok(u32::from_le_bytes(self.v()[0..4].to_array()))
    }

    fn read_i32(&self) -> anyhow::Result<i32> {
        anyhow::ensure!(self.v().len() >= 4);
        Ok(i32::from_le_bytes(self.v()[0..4].to_array()))
    }

    fn read_u64(&self) -> anyhow::Result<u64> {
        anyhow::ensure!(self.v().len() >= 8);
        Ok(u64::from_le_bytes(self.v()[0..8].to_array()))
    }

    fn read_i64(&self) -> anyhow::Result<i64> {
        anyhow::ensure!(self.v().len() >= 8);
        Ok(i64::from_le_bytes(self.v()[0..8].to_array()))
    }

    fn read_var_u32_len(&self) -> anyhow::Result<(u32, usize)> {
        let mut reader = self.v().limit_len(5);
        match leb128::read::unsigned(&mut reader) {
            Ok(v) => Ok((v as u32, 5 - reader.len())),
            Err(leb128::read::Error::Overflow) => {
                anyhow::bail!("LEB128-encoded u32 would overflow")
            }
            Err(leb128::read::Error::IoError(err)) if err.kind() == ErrorKind::UnexpectedEof => {
                anyhow::bail!("not enough bytes to read LEB128-encoded u32")
            }
            _ => unreachable!(),
        }
    }

    fn read_var_u32(&self) -> anyhow::Result<u32> {
        self.read_var_u32_len().map(|(v, _)| v)
    }

    fn read_var_i32_len(&self) -> anyhow::Result<(i32, usize)> {
        let mut reader = self.v().limit_len(5);
        match leb128::read::signed(&mut reader) {
            Ok(v) => Ok((v as i32, 5 - reader.len())),
            Err(leb128::read::Error::Overflow) => {
                anyhow::bail!("LEB128-encoded i32 would overflow")
            }
            Err(leb128::read::Error::IoError(err)) if err.kind() == ErrorKind::UnexpectedEof => {
                anyhow::bail!("not enough bytes to read LEB128-encoded i32")
            }
            _ => unreachable!(),
        }
    }

    fn read_var_i32(&self) -> anyhow::Result<i32> {
        self.read_var_i32_len().map(|(v, _)| v)
    }

    fn read_var_u64_len(&self) -> anyhow::Result<(u64, usize)> {
        let mut reader = self.v().limit_len(10);
        match leb128::read::unsigned(&mut reader) {
            Ok(v) => Ok((v, 10 - reader.len())),
            Err(leb128::read::Error::Overflow) => {
                anyhow::bail!("LEB128-encoded u64 would overflow")
            }
            _ => unreachable!(),
        }
    }

    fn read_var_u64(&self) -> anyhow::Result<u64> {
        self.read_var_u64_len().map(|(v, _)| v)
    }

    fn read_var_i64_len(&self) -> anyhow::Result<(i64, usize)> {
        let mut reader = self.v().limit_len(10);
        match leb128::read::signed(&mut reader) {
            Ok(v) => Ok((v, 10 - reader.len())),
            Err(leb128::read::Error::Overflow) => {
                anyhow::bail!("LEB128-encoded i64 would overflow")
            }
            _ => unreachable!(),
        }
    }

    fn read_var_i64(&self) -> anyhow::Result<i64> {
        self.read_var_i64_len().map(|(v, _)| v)
    }
}

impl Leb128ReadExt for [u8] {}

pub trait BufWriter {
    fn push(&mut self, v: u8) {
        self.extend(&[v]);
    }

    fn extend(&mut self, v: &[u8]);
}

impl BufWriter for Vec<u8> {
    fn push(&mut self, v: u8) {
        self.push(v)
    }

    fn extend(&mut self, v: &[u8]) {
        self.extend_from_slice(v)
    }
}

pub trait Leb128WriteExt: BufWriter {
    fn write_u8(&mut self, v: u8) {
        self.extend(&[v]);
    }

    fn write_u32(&mut self, v: u32) {
        self.extend(&v.to_le_bytes());
    }

    fn write_i32(&mut self, v: i32) {
        self.extend(&v.to_le_bytes());
    }

    fn write_u64(&mut self, v: u64) {
        self.extend(&v.to_le_bytes());
    }

    fn write_i64(&mut self, v: i64) {
        self.extend(&v.to_le_bytes());
    }

    fn write_var_u32(&mut self, v: u32) {
        let mut buf = [0u8; 5];
        let written = leb128::write::unsigned(&mut &mut buf[..], v.into()).unwrap();
        self.extend(&buf[0..written])
    }

    fn write_var_i32(&mut self, v: i32) {
        let mut buf = [0u8; 5];
        let written = leb128::write::signed(&mut &mut buf[..], v.into()).unwrap();
        self.extend(&buf[0..written])
    }

    fn write_var_u64(&mut self, v: u64) {
        let mut buf = [0u8; 10];
        let written = leb128::write::unsigned(&mut &mut buf[..], v).unwrap();
        self.extend(&buf[0..written])
    }

    fn write_var_i64(&mut self, v: i64) {
        let mut buf = [0u8; 10];
        let written = leb128::write::signed(&mut &mut buf[..], v).unwrap();
        self.extend(&buf[0..written])
    }
}

impl<E: ?Sized + BufWriter> Leb128WriteExt for E {}

#[derive(Debug, Clone, Default)]
pub struct LenCounter(pub usize);

impl BufWriter for LenCounter {
    fn extend(&mut self, v: &[u8]) {
        self.0 += v.len();
    }
}

pub fn len_of(f: impl FnOnce(&mut LenCounter)) -> usize {
    let mut lc = LenCounter::default();
    f(&mut lc);
    lc.0
}
