use anyhow::Context;
use wasmparser::{BinaryReader, FromReader, SectionLimited};

use crate::util::SliceExt;

// === Parsing === //

/// Parser for WASM relocation sections as described in the [WebAssembly Object File Linking](linking)
/// informal spec. I'm not exactly sure why the `"linking"` section is included in [`wasmparser`] but
/// the `"reloc."` section isn't.
///
/// [linking]: https://github.com/WebAssembly/tool-conventions/blob/4dd47d204df0c789c23d246bc4496631b5c199c4/Linking.md
#[derive(Debug, Clone)]
pub struct RelocSection<'a> {
    pub target_section: u32,
    pub entries: SectionLimited<'a, RelocEntry>,
}

impl<'a> FromReader<'a> for RelocSection<'a> {
    fn from_reader(reader: &mut BinaryReader<'a>) -> wasmparser::Result<Self> {
        Ok(Self {
            target_section: reader.read_var_u32()?,
            entries: {
                let start = reader.original_position();
                SectionLimited::new(reader.read_bytes(reader.bytes_remaining())?, start)?
            },
        })
    }
}

#[derive(Debug, Copy, Clone)]
pub struct RelocEntry {
    pub ty: Option<RelocEntryType>,
    pub offset: u32,
    pub index: u32,
    pub addend: Option<i32>,
}

impl<'a> FromReader<'a> for RelocEntry {
    fn from_reader(reader: &mut BinaryReader<'a>) -> wasmparser::Result<Self> {
        let ty = RelocEntryType::parse(reader.read_u8()?);
        let offset = reader.read_var_u32()?;
        let index = reader.read_var_u32()?;

        let addend = if ty.is_some_and(RelocEntryType::has_addend) {
            Some(reader.read_var_i32()?)
        } else {
            None
        };

        Ok(Self {
            ty,
            offset,
            index,
            addend,
        })
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum RelocEntryType {
    FunctionIndexLeb = 0,
    TableIndexSleb = 1,
    TableIndexI32 = 2,
    MemoryAddrLeb = 3,
    MemoryAddrSleb = 4,
    MemoryAddrI32 = 5,
    TypeIndexLeb = 6,
    GlobalIndexLeb = 7,
    FunctionOffsetI32 = 8,
    SectionOffsetI32 = 9,
    EventIndexLeb = 10,
    GlobalIndexI32 = 13,
}

impl RelocEntryType {
    pub fn parse(v: u8) -> Option<Self> {
        use RelocEntryType::*;

        Some(match v {
            0 => FunctionIndexLeb,
            1 => TableIndexSleb,
            2 => TableIndexI32,
            3 => MemoryAddrLeb,
            4 => MemoryAddrSleb,
            5 => MemoryAddrI32,
            6 => TypeIndexLeb,
            7 => GlobalIndexLeb,
            8 => FunctionOffsetI32,
            9 => SectionOffsetI32,
            10 => EventIndexLeb,
            13 => GlobalIndexI32,
            _ => return None,
        })
    }

    pub fn has_addend(self) -> bool {
        use RelocEntryType::*;

        matches!(
            self,
            MemoryAddrLeb | MemoryAddrSleb | MemoryAddrI32 | FunctionOffsetI32 | SectionOffsetI32
        )
    }

    pub fn rewrite_kind(self) -> ScalarRewriteKind {
        use {RelocEntryType::*, ScalarRewriteKind::*};

        match self {
            FunctionIndexLeb => VarU32,
            TableIndexSleb => VarI32,
            TableIndexI32 => U32,
            MemoryAddrLeb => VarU32,
            MemoryAddrSleb => VarI32,
            MemoryAddrI32 => U32,
            TypeIndexLeb => VarU32,
            GlobalIndexLeb => VarU32,
            FunctionOffsetI32 => U32,
            SectionOffsetI32 => U32,
            EventIndexLeb => VarU32,
            GlobalIndexI32 => U32,
        }
    }
}

// === Rewriting === //

pub fn rewrite_relocated(
    buf: &[u8],
    writer: &mut Vec<u8>,
    replacements: impl IntoIterator<Item = (usize, ScalarRewrite)>,
) -> anyhow::Result<()> {
    // Invariant: `buf_cursor` is always less than or equal to the `buf` length.
    let mut buf_cursor = 0;

    for (reloc_start, rewriter) in replacements {
        // While there are still relocations affecting bytes in our at the end of our buffer...
        debug_assert!(reloc_start >= buf_cursor);

        if reloc_start > buf.len() {
            break;
        }

        // Push the bytes up until the start of the relocation.
        writer.extend_from_slice(&buf[buf_cursor..reloc_start]);

        // Push the new relocation bytes.
        let reloc_end = reloc_start + rewriter.rewrite(&buf[reloc_start..], writer)?;

        // Bump the `buf_cursor`
        buf_cursor = reloc_end;
    }

    // Ensure that we write the remaining bytes of our buffer.
    writer.extend_from_slice(&buf[buf_cursor..]);

    Ok(())
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ScalarRewriteKind {
    VarU32,
    VarI32,
    VarU64,
    VarI64,
    U32,
    I32,
    U64,
    I64,
}

impl ScalarRewriteKind {
    // TODO: This should add support for offsets.
    pub fn read(self, buf: &[u8]) -> anyhow::Result<ScalarRewrite> {
        match self {
            Self::VarU32 => Self::read_var_u32(buf).map(ScalarRewrite::VarU32),
            Self::VarI32 => Self::read_var_i32(buf).map(ScalarRewrite::VarI32),
            Self::VarU64 => Self::read_var_u64(buf).map(ScalarRewrite::VarU64),
            Self::VarI64 => Self::read_var_i64(buf).map(ScalarRewrite::VarI64),
            Self::U32 => Self::read_u32(buf).map(ScalarRewrite::U32),
            Self::I32 => Self::read_i32(buf).map(ScalarRewrite::I32),
            Self::U64 => Self::read_u64(buf).map(ScalarRewrite::U64),
            Self::I64 => Self::read_i64(buf).map(ScalarRewrite::I64),
        }
    }

    pub fn read_var_u32(buf: &[u8]) -> anyhow::Result<u32> {
        leb128::read::unsigned(&mut buf.limit_len(5))
            .ok()
            .map(|v| v as u32)
            .context("failed to var u32")
    }

    pub fn read_var_i32(buf: &[u8]) -> anyhow::Result<i32> {
        leb128::read::signed(&mut buf.limit_len(5))
            .ok()
            .map(|v| v as i32)
            .context("failed to var i32")
    }

    pub fn read_var_u64(buf: &[u8]) -> anyhow::Result<u64> {
        leb128::read::unsigned(&mut buf.limit_len(10))
            .ok()
            .context("failed to var u64")
    }

    pub fn read_var_i64(buf: &[u8]) -> anyhow::Result<i64> {
        leb128::read::signed(&mut buf.limit_len(10))
            .ok()
            .context("failed to var i64")
    }

    pub fn read_u32(buf: &[u8]) -> anyhow::Result<u32> {
        anyhow::ensure!(buf.len() >= 4);
        Ok(u32::from_le_bytes(buf.to_array::<4>()))
    }

    pub fn read_i32(buf: &[u8]) -> anyhow::Result<i32> {
        anyhow::ensure!(buf.len() >= 4);
        Ok(i32::from_le_bytes(buf.to_array::<4>()))
    }

    pub fn read_u64(buf: &[u8]) -> anyhow::Result<u64> {
        anyhow::ensure!(buf.len() >= 8);
        Ok(u64::from_le_bytes(buf.to_array::<8>()))
    }

    pub fn read_i64(buf: &[u8]) -> anyhow::Result<i64> {
        anyhow::ensure!(buf.len() >= 8);
        Ok(i64::from_le_bytes(buf.to_array::<8>()))
    }

    pub fn as_zeroed(self) -> ScalarRewrite {
        match self {
            ScalarRewriteKind::VarU32 => ScalarRewrite::VarU32(0),
            ScalarRewriteKind::VarI32 => ScalarRewrite::VarI32(0),
            ScalarRewriteKind::VarU64 => ScalarRewrite::VarU64(0),
            ScalarRewriteKind::VarI64 => ScalarRewrite::VarI64(0),
            ScalarRewriteKind::U32 => ScalarRewrite::U32(0),
            ScalarRewriteKind::I32 => ScalarRewrite::I32(0),
            ScalarRewriteKind::U64 => ScalarRewrite::U64(0),
            ScalarRewriteKind::I64 => ScalarRewrite::I64(0),
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub enum ScalarRewrite {
    VarU32(u32),
    VarI32(i32),
    VarU64(u64),
    VarI64(i64),
    U32(u32),
    I32(i32),
    U64(u64),
    I64(i64),
}

impl ScalarRewrite {
    // TODO: This doesn't work well with signs, I think?
    pub fn as_u64(self) -> u64 {
        use ScalarRewrite::*;

        match self {
            VarU32(v) => v as u64,
            VarI32(v) => v as u64,
            VarU64(v) => v,
            VarI64(v) => v as u64,
            U32(v) => v as u64,
            I32(v) => v as u64,
            U64(v) => v,
            I64(v) => v as u64,
        }
    }

    pub fn kind(self) -> ScalarRewriteKind {
        match self {
            ScalarRewrite::VarU32(_) => ScalarRewriteKind::VarU32,
            ScalarRewrite::VarI32(_) => ScalarRewriteKind::VarI32,
            ScalarRewrite::VarU64(_) => ScalarRewriteKind::VarU64,
            ScalarRewrite::VarI64(_) => ScalarRewriteKind::VarI64,
            ScalarRewrite::U32(_) => ScalarRewriteKind::U32,
            ScalarRewrite::I32(_) => ScalarRewriteKind::I32,
            ScalarRewrite::U64(_) => ScalarRewriteKind::U64,
            ScalarRewrite::I64(_) => ScalarRewriteKind::I64,
        }
    }

    fn take_len_var(max_bytes: usize, buf: &[u8]) -> anyhow::Result<usize> {
        let mut i = 0;
        while i < max_bytes {
            anyhow::ensure!(buf.get(i).is_some());

            let no_cont = buf[i] & 0x80 == 0;

            if i == max_bytes - 1 {
                anyhow::ensure!(no_cont);
            }

            if no_cont {
                break;
            }

            i += 1;
        }

        Ok(i)
    }

    pub fn rewrite_var_u32(buf: &[u8], writer: &mut Vec<u8>, val: u32) -> anyhow::Result<usize> {
        let _ = leb128::write::unsigned(writer, val as u64);
        Self::take_len_var(5, buf)
    }

    pub fn rewrite_var_i32(buf: &[u8], writer: &mut Vec<u8>, val: i32) -> anyhow::Result<usize> {
        let _ = leb128::write::signed(writer, val as i64);
        Self::take_len_var(5, buf)
    }

    pub fn rewrite_var_u64(buf: &[u8], writer: &mut Vec<u8>, val: u64) -> anyhow::Result<usize> {
        let _ = leb128::write::unsigned(writer, val);
        Self::take_len_var(10, buf)
    }

    pub fn rewrite_var_i64(buf: &[u8], writer: &mut Vec<u8>, val: i64) -> anyhow::Result<usize> {
        let _ = leb128::write::signed(writer, val);
        Self::take_len_var(10, buf)
    }

    pub fn rewrite_u32(buf: &[u8], writer: &mut Vec<u8>, val: u32) -> anyhow::Result<usize> {
        anyhow::ensure!(buf.len() >= 4);
        writer.extend_from_slice(&val.to_le_bytes());
        Ok(4)
    }

    pub fn rewrite_i32(buf: &[u8], writer: &mut Vec<u8>, val: i32) -> anyhow::Result<usize> {
        anyhow::ensure!(buf.len() >= 4);
        writer.extend_from_slice(&val.to_le_bytes());
        Ok(4)
    }

    pub fn rewrite_u64(buf: &[u8], writer: &mut Vec<u8>, val: u64) -> anyhow::Result<usize> {
        anyhow::ensure!(buf.len() >= 8);
        writer.extend_from_slice(&val.to_le_bytes());
        Ok(8)
    }

    pub fn rewrite_i64(buf: &[u8], writer: &mut Vec<u8>, val: i64) -> anyhow::Result<usize> {
        anyhow::ensure!(buf.len() >= 8);
        writer.extend_from_slice(&val.to_le_bytes());
        Ok(8)
    }

    pub fn rewrite(self, buf: &[u8], writer: &mut Vec<u8>) -> anyhow::Result<usize> {
        use ScalarRewrite::*;

        match self {
            VarU32(val) => Self::rewrite_var_u32(buf, writer, val),
            VarI32(val) => Self::rewrite_var_i32(buf, writer, val),
            VarU64(val) => Self::rewrite_var_u64(buf, writer, val),
            VarI64(val) => Self::rewrite_var_i64(buf, writer, val),
            U32(val) => Self::rewrite_u32(buf, writer, val),
            I32(val) => Self::rewrite_i32(buf, writer, val),
            U64(val) => Self::rewrite_u64(buf, writer, val),
            I64(val) => Self::rewrite_i64(buf, writer, val),
        }
    }
}
