#![cfg(test)]

use super::*;

#[test]
fn prop_test_interrupted_stream() {
    let config = GearConfig::new(8192, 16384, 32768);
    let tables = GearTablesRef::new();

    let mut buffer = Vec::new();

    for _ in 0..config.max_size + 1 {
        let cuts_full = {
            let mut cuts = Vec::new();
            let mut state = GearState::new();

            let mut remaining = &buffer[..];

            while !remaining.is_empty() {
                let (consumed, cut) = state.push(&config, tables, remaining);
                if let Some(cut) = cut {
                    cuts.push(cut);
                }
                remaining = &remaining[consumed..];
            }

            cuts.push(state.reset());

            cuts
        };

        let cuts_split = {
            let mut cuts = Vec::new();
            let mut state = GearState::new();

            let mut remaining = &buffer[..];

            while !remaining.is_empty() {
                let (consumed, cut) = state.push(
                    &config,
                    tables,
                    &remaining[..fastrand::usize(0..=remaining.len())],
                );
                if let Some(cut) = cut {
                    cuts.push(cut);
                }
                remaining = &remaining[consumed..];
            }

            cuts.push(state.reset());

            cuts
        };

        assert_eq!(cuts_full, cuts_split);

        buffer.push(fastrand::u8(..));
    }
}
