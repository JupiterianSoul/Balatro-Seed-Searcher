//! Per-run game-state derivations.
//!
//! Balatro's deterministic surface (the bits seed searchers care about) is
//! reachable from the seed alone, _given_ the run config (deck, stake, gold,
//! voucher chain). This module owns the "what does ante N look like" math:
//!
//!   - Shop slots (jokers / tarots / planets / spectrals + their editions)
//!   - Booster pack contents
//!   - Tag chain (skip rewards)
//!   - Boss blind selection
//!   - Voucher slot
//!   - Soul card targeting (legendary jokers)
//!
//! The current build provides the data scaffolding and the most-queried
//! derivations. Less-common sources (planet packs, standard packs full
//! contents, sigil/ouija outputs) are filled in incrementally — each one
//! is a small, isolated PR against this file.

use crate::rng::{pseudohash, pseudoseed_random, LuaRandom};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Deck {
    Red, Blue, Yellow, Green, Black, Magic, Nebula, Ghost, Abandoned,
    Checkered, Zodiac, Painted, Anaglyph, Plasma, Erratic,
}

/// Stakes in ascending difficulty order. The discriminants are used by the
/// sticker-roll logic to gate eternal/perishable/rental drops (e.g. eternal
/// requires Black stake or higher).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Stake {
    White = 1,
    Red = 2,
    Green = 3,
    Black = 4,
    Blue = 5,
    Purple = 6,
    Orange = 7,
    Gold = 8,
}

#[derive(Clone, Debug)]
pub struct RunConfig {
    pub deck: Deck,
    pub stake: Stake,
    pub seed: [u8; 8],
    pub seed_len: u8,
}

/// Helper that materialises the seed bytes back into a borrowed `&str`
/// for hashing without allocating.
pub fn seed_as_str(buf: &[u8; 8], len: u8) -> &str {
    // SAFETY: writers only ever put ASCII-alphabet chars in here.
    unsafe { core::str::from_utf8_unchecked(&buf[..len as usize]) }
}

/// Produces the RNG used for picking jokers at a given (ante, slot) location.
/// Source key naming follows Balatro's convention: e.g. `"Joker1"`, `"Joker2"`.
#[inline]
pub fn rng_for_source(seed: &str, source: &str) -> LuaRandom {
    pseudoseed_random(seed, source)
}

/// Convenience: `pseudohash(key + seed)` for callers that just need the
/// raw deterministic double without going through `LuaRandom`.
#[inline]
pub fn keyed_hash(seed: &str, key: &str) -> f64 {
    let mut s = String::with_capacity(seed.len() + key.len());
    s.push_str(key);
    s.push_str(seed);
    pseudohash(&s)
}
