#![cfg(test)]

use super::*;

#[test]
fn prop_test_stability() {
    let mut remaining = &include_bytes!("../test/SekienAkashita.jpg")[..];

    let config = GearConfig::new(8192, 16384, 32768);
    let tables = GearTablesRef::new();
    let mut state = GearState::new();

    let mut cuts = Vec::new();

    while !remaining.is_empty() {
        let (consumed, cut) = state.push(&config, tables, remaining);

        if let Some(cut) = cut {
            cuts.push(cut);
        }

        remaining = &remaining[consumed..];
    }

    cuts.push(state.reset());

    assert_eq!(
        cuts,
        &[
            GearState {
                hash: 17968276318003433923,
                len: 21326
            },
            GearState {
                hash: 4098594969649699419,
                len: 17140
            },
            GearState {
                hash: 15733367461443853673,
                len: 28084
            },
            GearState {
                hash: 9018472446127356606,
                len: 18217
            },
            GearState {
                hash: 5008929482200865166,
                len: 24699
            }
        ]
    );
}

#[test]
fn prop_test_interrupted_stream() {
    fastrand::seed(4);

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
