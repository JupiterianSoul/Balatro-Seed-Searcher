//! Thin wasm-bindgen surface — the only thing the JS workers see.
//!
//! Designed for SharedArrayBuffer-friendly streaming: workers call
//! `scan_chunk`, get back a packed `Uint8Array` of (rank: u64, score: u8,
//! seed: 8 bytes) tuples. No per-match JS allocation.

use wasm_bindgen::prelude::*;
use crate::filter::Filter;
use crate::search::Searcher;
use crate::state::{Deck, Stake, RunConfig};

#[wasm_bindgen]
pub fn init() {
    #[cfg(feature = "wasm")]
    console_error_panic_hook::set_once();
}

/// Scan a chunk and return packed match records.
/// Each record: 8 bytes rank LE + 1 byte score + 8 bytes seed (right-padded).
/// = 17 bytes per match.
#[wasm_bindgen]
pub fn scan_chunk(
    filter_json: &str,
    start_rank: u64,
    count: u64,
    seed_len: u8,
    deck_idx: u8,
    stake_idx: u8,
    partial: bool,
    min_score: u8,
) -> Vec<u8> {
    let filter: Filter = serde_json::from_str(filter_json).unwrap_or_default();
    let compiled = filter.compile();

    let config = RunConfig {
        deck: deck_from_idx(deck_idx),
        stake: stake_from_idx(stake_idx),
        seed: [0; 8],
        seed_len,
    };

    let s = Searcher {
        config: &config,
        filter: &compiled,
        partial,
        min_score,
    };

    let mut out: Vec<u8> = Vec::with_capacity(64 * 17);
    s.scan(start_rank, count, |m| {
        out.extend_from_slice(&m.rank.to_le_bytes());
        out.push(m.score);
        let mut buf = [b' '; 8];
        let b = m.seed.as_bytes();
        buf[..b.len()].copy_from_slice(b);
        out.extend_from_slice(&buf);
    });
    out
}

fn deck_from_idx(i: u8) -> Deck {
    match i {
        0 => Deck::Red, 1 => Deck::Blue, 2 => Deck::Yellow, 3 => Deck::Green,
        4 => Deck::Black, 5 => Deck::Magic, 6 => Deck::Nebula, 7 => Deck::Ghost,
        8 => Deck::Abandoned, 9 => Deck::Checkered, 10 => Deck::Zodiac,
        11 => Deck::Painted, 12 => Deck::Anaglyph, 13 => Deck::Plasma,
        _ => Deck::Erratic,
    }
}
fn stake_from_idx(i: u8) -> Stake {
    match i {
        0 => Stake::White, 1 => Stake::Red, 2 => Stake::Green, 3 => Stake::Black,
        4 => Stake::Blue, 5 => Stake::Purple, 6 => Stake::Orange, _ => Stake::Gold,
    }
}
