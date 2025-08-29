// Adapted from: https://github.com/nlfiedler/fastcdc-rs/blob/49c3d0b8043a7c1c2d9aca75e868d3791ffedcf3/src/v2020/mod.rs
//   Copyright (c) 2025 Nathan Fiedler

#![cfg(test)]

use super::*;

use md5::{Digest, Md5};

use std::fs;

fn cut_model(config: &GearConfig, tables: GearTablesRef, source: &[u8]) -> (u64, usize) {
    let GearConfig {
        min_size,
        avg_size,
        max_size,
        mask_s,
        mask_l,
        mask_s_ls,
        mask_l_ls,
    } = *config;
    let GearTablesRef { gear, gear_ls } = tables;

    fastcdc::v2020::cut_gear(
        source, min_size, avg_size, max_size, mask_s, mask_l, mask_s_ls, mask_l_ls, gear, gear_ls,
    )
}

fn cut_impl(config: &GearConfig, tables: GearTablesRef, source: &[u8]) -> (u64, usize) {
    let mut state = GearState::default();
    state.push(config, tables, source).map_or(
        (state.hash, source.len()),
        |(count, GearState { hash, .. })| (hash, count),
    )
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct FastCDC<'a> {
    source: &'a [u8],
    processed: usize,
    remaining: usize,

    config: GearConfig,
    tables: GearTables,
}

impl<'a> FastCDC<'a> {
    fn new(source: &'a [u8], min_size: u32, avg_size: u32, max_size: u32) -> Self {
        Self::with_level(source, min_size, avg_size, max_size, Normalization::Level1)
    }

    fn with_level(
        source: &'a [u8],
        min_size: u32,
        avg_size: u32,
        max_size: u32,
        level: Normalization,
    ) -> Self {
        Self::with_level_and_seed(source, min_size, avg_size, max_size, level, 0)
    }

    fn with_level_and_seed(
        source: &'a [u8],
        min_size: u32,
        avg_size: u32,
        max_size: u32,
        level: Normalization,
        seed: u64,
    ) -> Self {
        Self {
            source,
            processed: 0,
            remaining: source.len(),
            config: GearConfig::with_level(min_size, avg_size, max_size, level),
            tables: GearTables::new(seed),
        }
    }

    /// Find the next cut point in the data, where `start` is the position from
    /// which to start processing the source data, and `remaining` are the
    /// number of bytes left to be processed.
    ///
    /// The returned 2-tuple consists of the 64-bit hash (fingerprint) and the
    /// byte offset of the end of the chunk. Note that the hash values may
    /// differ from those produced by the v2016 chunker.
    ///
    /// There is a special case in which the remaining bytes are less than the
    /// minimum chunk size, at which point this function returns a hash of 0 and
    /// the cut point is the end of the source data.
    fn cut(&self, start: usize, remaining: usize) -> (u64, usize) {
        let (hash, count) = cut_impl(
            &self.config,
            self.tables.get(),
            &self.source[start..][..remaining],
        );

        (hash, start + count)
    }
}

impl Iterator for FastCDC<'_> {
    type Item = Chunk;

    fn next(&mut self) -> Option<Chunk> {
        if self.remaining == 0 {
            None
        } else {
            let (hash, cutpoint) = self.cut(self.processed, self.remaining);
            if cutpoint == 0 {
                None
            } else {
                let offset = self.processed;
                let length = cutpoint - offset;
                self.processed += length;
                self.remaining -= length;
                Some(Chunk {
                    hash,
                    offset,
                    length,
                })
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let upper_bound = self.remaining / self.config.min_size;
        (upper_bound, Some(upper_bound))
    }
}

/// Represents a chunk returned from the [`FastCDC`] iterator.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
struct Chunk {
    /// The gear hash value as of the end of the chunk.
    hash: u64,

    /// Starting byte position within the source.
    offset: usize,

    /// Length of the chunk in bytes.
    length: usize,
}

#[test]
#[should_panic]
fn test_minimum_too_low() {
    GearConfig::new(63, 256, 1024);
}

#[test]
#[should_panic]
fn test_minimum_too_high() {
    GearConfig::new(67_108_867, 256, 1024);
}

#[test]
#[should_panic]
fn test_average_too_low() {
    GearConfig::new(64, 255, 1024);
}

#[test]
#[should_panic]
fn test_average_too_high() {
    GearConfig::new(64, 268_435_457, 1024);
}

#[test]
#[should_panic]
fn test_maximum_too_low() {
    GearConfig::new(64, 256, 1023);
}

#[test]
#[should_panic]
fn test_maximum_too_high() {
    GearConfig::new(64, 256, 1_073_741_825);
}

#[test]
fn test_masks() {
    let config = GearConfig::new(64, 256, 1024);
    assert_eq!(config.mask_l, MASKS[7]);
    assert_eq!(config.mask_s, MASKS[9]);

    let config = GearConfig::new(8192, 16384, 32768);
    assert_eq!(config.mask_l, MASKS[13]);
    assert_eq!(config.mask_s, MASKS[15]);

    let config = GearConfig::new(1_048_576, 4_194_304, 16_777_216);
    assert_eq!(config.mask_l, MASKS[21]);
    assert_eq!(config.mask_s, MASKS[23]);
}

#[test]
fn test_cut_all_zeros() {
    // for all zeros, always returns chunks of maximum size
    let array = [0u8; 10240];
    let chunker = FastCDC::new(&array, 64, 256, 1024);
    let mut cursor: usize = 0;
    for _ in 0..10 {
        let (hash, pos) = chunker.cut(cursor, 10240 - cursor);
        assert_eq!(hash, 14169102344523991076);
        assert_eq!(pos, cursor + 1024);
        cursor = pos;
    }
    // assert that nothing more should be returned
    let (_, pos) = chunker.cut(cursor, 10240 - cursor);
    assert_eq!(pos, 10240);
}

#[test]
fn test_cut_sekien_16k_chunks() {
    let read_result = fs::read("test/SekienAkashita.jpg");
    assert!(read_result.is_ok());
    let contents = read_result.unwrap();
    let chunker = FastCDC::new(&contents, 4096, 16384, 65535);
    let mut cursor: usize = 0;
    let mut remaining: usize = contents.len();
    let expected: Vec<(u64, usize)> = vec![
        (17968276318003433923, 21325),
        (8197189939299398838, 17140),
        (13019990849178155730, 28084),
        (4509236223063678303, 18217),
        (2504464741100432583, 24700),
    ];
    for (e_hash, e_length) in expected.iter() {
        let (hash, pos) = chunker.cut(cursor, remaining);
        assert_eq!(hash, *e_hash);
        assert_eq!(pos, cursor + e_length);
        cursor = pos;
        remaining -= e_length;
    }
    assert_eq!(remaining, 0);
}

#[test]
fn test_cut_sekien_16k_chunks_seed_666() {
    let read_result = fs::read("test/SekienAkashita.jpg");
    assert!(read_result.is_ok());
    let contents = read_result.unwrap();
    let chunker =
        FastCDC::with_level_and_seed(&contents, 4096, 16384, 65535, Normalization::Level1, 666);
    let mut cursor: usize = 0;
    let mut remaining: usize = contents.len();
    let expected: Vec<(u64, usize)> = vec![
        (9312357714466240148, 10605),
        (226910853333574584, 55745),
        (12271755243986371352, 11346),
        (14153975939352546047, 5883),
        (5890158701071314778, 11586),
        (8981594897574481255, 14301),
    ];
    for (e_hash, e_length) in expected.iter() {
        let (hash, pos) = chunker.cut(cursor, remaining);
        assert_eq!(hash, *e_hash);
        assert_eq!(pos, cursor + e_length);
        cursor = pos;
        remaining -= e_length;
    }
    assert_eq!(remaining, 0);
}

#[test]
fn test_cut_sekien_32k_chunks() {
    let read_result = fs::read("test/SekienAkashita.jpg");
    assert!(read_result.is_ok());
    let contents = read_result.unwrap();
    let chunker = FastCDC::new(&contents, 8192, 32768, 131072);
    let mut cursor: usize = 0;
    let mut remaining: usize = contents.len();
    let expected: Vec<(u64, usize)> =
        vec![(15733367461443853673, 66549), (6321136627705800457, 42917)];
    for (e_hash, e_length) in expected.iter() {
        let (hash, pos) = chunker.cut(cursor, remaining);
        assert_eq!(hash, *e_hash);
        assert_eq!(pos, cursor + e_length);
        cursor = pos;
        remaining -= e_length;
    }
    assert_eq!(remaining, 0);
}

#[test]
fn test_cut_sekien_64k_chunks() {
    let read_result = fs::read("test/SekienAkashita.jpg");
    assert!(read_result.is_ok());
    let contents = read_result.unwrap();
    let chunker = FastCDC::new(&contents, 16384, 65536, 262144);
    let mut cursor: usize = 0;
    let mut remaining: usize = contents.len();
    let expected: Vec<(u64, usize)> = vec![(2504464741100432583, 109466)];
    for (e_hash, e_length) in expected.iter() {
        let (hash, pos) = chunker.cut(cursor, remaining);
        assert_eq!(hash, *e_hash);
        assert_eq!(pos, cursor + e_length);
        cursor = pos;
        remaining -= e_length;
    }
    assert_eq!(remaining, 0);
}

struct ExpectedChunk {
    hash: u64,
    offset: u64,
    length: usize,
    digest: String,
}

#[test]
fn test_iter_sekien_16k_chunks() {
    let read_result = fs::read("test/SekienAkashita.jpg");
    assert!(read_result.is_ok());
    let contents = read_result.unwrap();
    // The digest values are not needed here, but they serve to validate
    // that the streaming version tested below is returning the correct
    // chunk data on each iteration.
    let expected_chunks = vec![
        ExpectedChunk {
            hash: 17968276318003433923,
            offset: 0,
            length: 21325,
            digest: "2bb52734718194617c957f5e07ee6054".into(),
        },
        ExpectedChunk {
            hash: 8197189939299398838,
            offset: 21325,
            length: 17140,
            digest: "badfb0757fe081c20336902e7131f768".into(),
        },
        ExpectedChunk {
            hash: 13019990849178155730,
            offset: 38465,
            length: 28084,
            digest: "18412d7414de6eb42f638351711f729d".into(),
        },
        ExpectedChunk {
            hash: 4509236223063678303,
            offset: 66549,
            length: 18217,
            digest: "04fe1405fc5f960363bfcd834c056407".into(),
        },
        ExpectedChunk {
            hash: 2504464741100432583,
            offset: 84766,
            length: 24700,
            digest: "1aa7ad95f274d6ba34a983946ebc5af3".into(),
        },
    ];
    let chunker = FastCDC::new(&contents, 4096, 16384, 65535);
    let mut index = 0;
    for chunk in chunker {
        assert_eq!(chunk.hash, expected_chunks[index].hash);
        assert_eq!(chunk.offset, expected_chunks[index].offset as usize);
        assert_eq!(chunk.length, expected_chunks[index].length);
        let mut hasher = Md5::new();
        hasher.update(&contents[chunk.offset..chunk.offset + chunk.length]);
        let table = hasher.finalize();
        let digest = format!("{:x}", table);
        assert_eq!(digest, expected_chunks[index].digest);
        index += 1;
    }
    assert_eq!(index, 5);
}

#[test]
fn test_cut_sekien_16k_nc_0() {
    let read_result = fs::read("test/SekienAkashita.jpg");
    assert!(read_result.is_ok());
    let contents = read_result.unwrap();
    let chunker = FastCDC::with_level(&contents, 4096, 16384, 65535, Normalization::Level0);
    let mut cursor: usize = 0;
    let mut remaining: usize = contents.len();
    let expected: Vec<(u64, usize)> = vec![
        (443122261039895162, 6634),
        (15733367461443853673, 59915),
        (10460176299449652894, 25597),
        (6197802202431009942, 5237),
        (6321136627705800457, 12083),
    ];
    for (e_hash, e_length) in expected.iter() {
        let (hash, pos) = chunker.cut(cursor, remaining);
        assert_eq!(hash, *e_hash);
        assert_eq!(pos, cursor + e_length);
        cursor = pos;
        remaining -= e_length;
    }
    assert_eq!(remaining, 0);
}

#[test]
fn test_cut_sekien_16k_nc_3() {
    let read_result = fs::read("test/SekienAkashita.jpg");
    assert!(read_result.is_ok());
    let contents = read_result.unwrap();
    let chunker = FastCDC::with_level(&contents, 8192, 16384, 32768, Normalization::Level3);
    let mut cursor: usize = 0;
    let mut remaining: usize = contents.len();
    let expected: Vec<(u64, usize)> = vec![
        (10718006254707412376, 17350),
        (13104072099671895560, 19911),
        (12322483109039221194, 17426),
        (16009206469796846404, 17519),
        (2473608525189754172, 19940),
        (2504464741100432583, 17320),
    ];
    for (e_hash, e_length) in expected.iter() {
        let (hash, pos) = chunker.cut(cursor, remaining);
        assert_eq!(hash, *e_hash);
        assert_eq!(pos, cursor + e_length);
        cursor = pos;
        remaining -= e_length;
    }
    assert_eq!(remaining, 0);
}

#[test]
fn prop_check_gear_state_random() {
    fastrand::seed(5);

    let config = GearConfig::new(8192, 16384, 32768);
    let tables = GearTablesRef::new();

    let mut buffer = Vec::new();

    for _ in 0..config.max_size {
        buffer.push(fastrand::u8(..));

        // `cut_model` only ever updates the hash once it has a pair of bytes available but we
        // process the stream byte-by-byte.
        if buffer
            .len()
            .checked_sub(config.min_size + 1)
            .is_some_and(|v| v % 2 == 0)
        {
            continue;
        }

        assert_eq!(
            cut_model(&config, tables, &buffer),
            cut_impl(&config, tables, &buffer),
        );
    }
}

#[test]
fn prop_check_gear_state_zeroes() {
    fastrand::seed(5);

    let config = GearConfig::new(8192, 16384, 32768);
    let tables = GearTablesRef::new();

    let mut buffer = Vec::new();

    for _ in 0..config.max_size {
        buffer.push(0u8);

        // `cut_model` only ever updates the hash once it has a pair of bytes available but we
        // process the stream byte-by-byte.
        if buffer
            .len()
            .checked_sub(config.min_size + 1)
            .is_some_and(|v| v % 2 == 0)
        {
            continue;
        }

        assert_eq!(
            cut_model(&config, tables, &buffer),
            cut_impl(&config, tables, &buffer),
        );
    }
}
