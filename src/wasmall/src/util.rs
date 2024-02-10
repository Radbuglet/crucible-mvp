use core::fmt;
use std::{
    any::type_name,
    collections::BTreeMap,
    io::ErrorKind,
    marker::PhantomData,
    mem,
    ops::Range,
    sync::{Mutex, MutexGuard},
};

use anyhow::Context;

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

// === OffsetTracker === //

#[derive(Debug)]
pub struct OffsetTracker<'a> {
    _ty: PhantomData<&'a [()]>,
    range: Range<usize>,
}

impl OffsetTracker<'_> {
    // Maps tracked slice starts to slice ends.
    fn get_slices() -> MutexGuard<'static, BTreeMap<usize, usize>> {
        static SLICES: Mutex<BTreeMap<usize, usize>> = Mutex::new(BTreeMap::new());
        match SLICES.lock() {
            Ok(guard) => guard,
            Err(guard) => guard.into_inner(),
        }
    }

    pub fn new_raw(range: Range<usize>) -> Self {
        let mut slices = Self::get_slices();

        // Ensure that the range is properly formed.
        assert!(range.start <= range.end);

        // See if there are any ranges that start before our end.
        if let Some((_, &closest_end)) = slices.range(..range.end).next() {
            // If there is, ensure that it ends before we begin. This is sufficient to ensure that
            // all ranges starting before us don't overlap with us because all subsequent range ends
            // are less than or equal to this range's end by the no-overlap invariant.
            assert!(closest_end <= range.start);
        }

        // See if we're about to collide into the start of the next range.
        if let Some((&closest_start, _)) = slices.range(range.start..).next() {
            // ibid
            assert!(range.end <= closest_start);
        }

        // Our range does not overlap so let's insert it.
        slices.insert(range.start, range.end);

        Self {
            _ty: PhantomData,
            range,
        }
    }

    pub fn lookup_parent_raw(addr: usize) -> Option<Range<usize>> {
        Self::get_slices()
            .range(..addr)
            .next()
            .map(|(&start, &end)| start..end)
            .filter(|range| range.contains(&addr))
    }

    pub fn cast_lifetime<'b>(self) -> OffsetTracker<'b> {
        let range = self.range.clone();
        std::mem::forget(self);
        OffsetTracker {
            _ty: PhantomData,
            range,
        }
    }
}

impl Drop for OffsetTracker<'_> {
    fn drop(&mut self) {
        Self::get_slices().remove(&self.range.start);
    }
}

// High-level interface
impl OffsetTracker<'_> {
    pub fn new<T>(slice: &[T]) -> Self {
        let range = slice.as_ptr_range();
        let range = (range.start as usize)..(range.end as usize);
        Self::new_raw(range)
    }

    pub fn index_in_parent<T>(value: *const T) -> Option<usize> {
        let value = value as usize;
        Self::lookup_parent_raw(value).map(|range| (value - range.start) / mem::size_of::<T>())
    }
}

pub struct FmtOffset<T>(pub *const T);

impl<T> Copy for FmtOffset<T> {}

impl<T> Clone for FmtOffset<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> fmt::Display for FmtOffset<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(offset) = OffsetTracker::index_in_parent(self.0) {
            offset.fmt(f)
        } else {
            f.write_str("offset unavailable")
        }
    }
}

// === Reading === //

// ByteCursor
#[derive(Debug, Clone)]
pub struct ByteCursor<'a>(pub &'a [u8]);

impl<'a> ByteCursor<'a> {
    // Debug
    pub fn global_offset(&self) -> FmtOffset<u8> {
        FmtOffset(self.0.as_ptr())
    }

    // Primitives
    pub fn at_eof(&self) -> bool {
        self.0.is_empty()
    }

    pub fn peek(&self, count: usize) -> anyhow::Result<&'a [u8]> {
        anyhow::ensure!(
            self.0.len() >= count,
            "failed to read {count} byte{} at position {}",
            if count == 1 { "" } else { "s" },
            self.global_offset(),
        );
        Ok(&self.0[0..count])
    }

    pub fn consume(&mut self, count: usize) -> anyhow::Result<&'a [u8]> {
        anyhow::ensure!(
            self.0.len() >= count,
            "failed to read {count} byte{} at position {}",
            if count == 1 { "" } else { "s" },
            self.global_offset(),
        );

        let (read, remainder) = self.0.split_at(count);
        self.0 = remainder;
        Ok(read)
    }

    pub fn consume_arr<const N: usize>(&mut self) -> anyhow::Result<[u8; N]> {
        self.consume(N).map(|v| v.to_array())
    }

    pub fn advance(&mut self, count: usize) {
        self.consume(count).unwrap();
    }

    pub fn lookahead<R>(
        &mut self,
        f: impl FnOnce(&mut Self) -> anyhow::Result<R>,
    ) -> anyhow::Result<R> {
        let mut fork = self.clone();
        let res = f(&mut fork);
        if res.is_ok() {
            *self = fork;
        }
        res
    }

    pub fn lookahead_annotated<R>(
        &mut self,
        what: impl fmt::Display,
        f: impl FnOnce(&mut Self) -> anyhow::Result<R>,
    ) -> anyhow::Result<R> {
        let start = self.global_offset();
        self.lookahead(f)
            .map_err(|err| err.context(format!("failed to parse {what} starting at {start}")))
    }

    // Specified readers
    pub fn read_u8(&mut self) -> anyhow::Result<u8> {
        self.consume_arr().map(u8::from_le_bytes)
    }

    pub fn read_u32(&mut self) -> anyhow::Result<u32> {
        self.consume_arr().map(u32::from_le_bytes)
    }

    pub fn read_i32(&mut self) -> anyhow::Result<i32> {
        self.consume_arr().map(i32::from_le_bytes)
    }

    pub fn read_u64(&mut self) -> anyhow::Result<u64> {
        self.consume_arr().map(u64::from_le_bytes)
    }

    pub fn read_i64(&mut self) -> anyhow::Result<i64> {
        self.consume_arr().map(i64::from_le_bytes)
    }

    pub fn read_var_u32(&mut self) -> anyhow::Result<u32> {
        let mut reader = self.0.limit_len(5);

        match leb128::read::unsigned(&mut reader) {
            Ok(v) => {
                self.advance(5 - reader.len());
                Ok(v as u32)
            }
            Err(leb128::read::Error::Overflow) => Err(anyhow::anyhow!(
                "LEB128-encoded `u32` starting at {} would overflow",
                self.global_offset()
            )),
            Err(leb128::read::Error::IoError(err)) if err.kind() == ErrorKind::UnexpectedEof => {
                Err(anyhow::anyhow!(
                    "not enough bytes to read LEB128-encoded `u32` starting at {}",
                    self.global_offset()
                ))
            }
            _ => unreachable!(),
        }
    }

    pub fn read_var_i32(&mut self) -> anyhow::Result<i32> {
        let mut reader = self.0.limit_len(5);

        match leb128::read::signed(&mut reader) {
            Ok(v) => {
                self.advance(5 - reader.len());
                Ok(v as i32)
            }
            Err(leb128::read::Error::Overflow) => Err(anyhow::anyhow!(
                "LEB128-encoded `i32` starting at {} would overflow",
                self.global_offset()
            )),
            Err(leb128::read::Error::IoError(err)) if err.kind() == ErrorKind::UnexpectedEof => {
                Err(anyhow::anyhow!(
                    "not enough bytes to read LEB128-encoded `i32` starting at {}",
                    self.global_offset()
                ))
            }
            _ => unreachable!(),
        }
    }

    pub fn read_var_u64(&mut self) -> anyhow::Result<u64> {
        let mut reader = self.0.limit_len(10);

        match leb128::read::unsigned(&mut reader) {
            Ok(v) => {
                self.advance(10 - reader.len());
                Ok(v)
            }
            Err(leb128::read::Error::Overflow) => Err(anyhow::anyhow!(
                "LEB128-encoded `u64` starting at {} would overflow",
                self.global_offset()
            )),
            Err(leb128::read::Error::IoError(err)) if err.kind() == ErrorKind::UnexpectedEof => {
                Err(anyhow::anyhow!(
                    "not enough bytes to read LEB128-encoded `u64` starting at {}",
                    self.global_offset()
                ))
            }
            _ => unreachable!(),
        }
    }

    pub fn read_var_i64(&mut self) -> anyhow::Result<i64> {
        let mut reader = self.0.limit_len(10);

        match leb128::read::signed(&mut reader) {
            Ok(v) => {
                self.advance(10 - reader.len());
                Ok(v)
            }
            Err(leb128::read::Error::Overflow) => Err(anyhow::anyhow!(
                "LEB128-encoded `i64` starting at {} would overflow",
                self.global_offset()
            )),
            Err(leb128::read::Error::IoError(err)) if err.kind() == ErrorKind::UnexpectedEof => {
                Err(anyhow::anyhow!(
                    "not enough bytes to read LEB128-encoded `i64` starting at {}",
                    self.global_offset()
                ))
            }
            _ => unreachable!(),
        }
    }
}

// ByteSliceExt
pub trait ByteSliceExt: ExtensionFor<[u8]> {
    fn try_count_bytes_read(
        &self,
        f: impl FnOnce(&mut ByteCursor<'_>) -> anyhow::Result<()>,
    ) -> anyhow::Result<usize> {
        let mut buf = ByteCursor(self.v());
        f(&mut buf)?;
        Ok(self.v().len() - buf.0.len())
    }

    fn count_bytes_read(&self, f: impl FnOnce(&mut ByteCursor<'_>)) -> usize {
        let mut buf = ByteCursor(self.v());
        f(&mut buf);
        self.v().len() - buf.0.len()
    }
}

impl ByteSliceExt for [u8] {}

// ByteParse
pub trait ByteParse<'a>: Sized {
    type Out;

    fn parse(buf: &mut ByteCursor<'a>) -> anyhow::Result<Self::Out> {
        buf.lookahead_annotated(type_name::<Self>(), Self::parse_naked)
    }

    fn parse_naked(buf: &mut ByteCursor<'a>) -> anyhow::Result<Self::Out>;
}

pub struct ByteParseList<'a, P> {
    _ty: PhantomData<fn() -> P>,
    cursor: ByteCursor<'a>,
}

impl<'a, P> ByteParseList<'a, P> {
    pub fn new(cursor: ByteCursor<'a>) -> Self {
        Self {
            _ty: PhantomData,
            cursor,
        }
    }

    pub fn cursor(&self) -> ByteCursor<'a> {
        self.cursor.clone()
    }
}

impl<'a, P: ByteParse<'a>> Iterator for ByteParseList<'a, P> {
    type Item = anyhow::Result<P::Out>;

    fn next(&mut self) -> Option<Self::Item> {
        (!self.cursor.at_eof()).then(|| P::parse(&mut self.cursor))
    }
}

impl<'a, P> fmt::Debug for ByteParseList<'a, P>
where
    P: ByteParse<'a>,
    P::Out: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut f = f.debug_list();

        for item in self.clone() {
            match item {
                Ok(v) => {
                    f.entry(&v);
                }
                Err(err) => {
                    f.entry(&err);
                    break;
                }
            }
        }

        f.finish()
    }
}

impl<P> Clone for ByteParseList<'_, P> {
    fn clone(&self) -> Self {
        Self {
            _ty: PhantomData,
            cursor: self.cursor.clone(),
        }
    }
}

#[non_exhaustive]
pub struct VarU32;

impl ByteParse<'_> for VarU32 {
    type Out = u32;

    fn parse_naked(buf: &mut ByteCursor<'_>) -> anyhow::Result<Self::Out> {
        buf.read_var_u32()
    }
}

#[non_exhaustive]
pub struct VarI32;

impl ByteParse<'_> for VarI32 {
    type Out = i32;

    fn parse_naked(buf: &mut ByteCursor<'_>) -> anyhow::Result<Self::Out> {
        buf.read_var_i32()
    }
}

#[non_exhaustive]
pub struct VarByteVec;

impl<'a> ByteParse<'a> for VarByteVec {
    type Out = &'a [u8];

    fn parse_naked(buf: &mut ByteCursor<'a>) -> anyhow::Result<Self::Out> {
        let len = buf.read_var_u32().context("failed to read length")?;
        buf.consume(len as usize)
    }
}

// === Writing === //

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
