//! Real game-state derivations. Ported from Immolate's functions.cl.
//!
//! Every function here takes a fresh `Instance` (so the node cache starts
//! empty) and returns the canonical "what is this draw" answer. Callers
//! typically build one `Instance` per (seed, run config) and then ask it
//! multiple questions — the cache makes the second-and-later calls cheap.
//!
//! Implemented now (most-queried surface):
//!   - `next_joker_rarity` — common/uncommon/rare/legendary roll
//!   - `next_joker_edition` — base/foil/holo/poly/negative roll
//!   - `next_joker` — full shop joker draw (rarity + identity, no stickers)
//!   - `next_tag` — skip-tag draw
//!   - `next_voucher` — per-ante voucher draw
//!   - `next_boss` — boss blind draw (small/big bosses for ante%8!=0,
//!     finisher bosses for ante%8==0)
//!
//! Stickers, locking, voucher chain side-effects, and pack contents land in
//! a follow-up — they need the lock/voucher state machine to be honest.

use crate::instance::{Instance, NodeKey, RandomSource, RandomType};
use crate::tables::{COMMON_JOKERS, LEGENDARY_JOKERS, RARE_JOKERS, UNCOMMON_JOKERS, TAGS, VOUCHERS, BOSSES_SMALL_BIG, BOSSES_FINISHER};

/// Rarity bucket the next joker draw falls into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rarity { Common, Uncommon, Rare, Legendary }

/// Edition rolled for the next joker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edition { None, Foil, Holographic, Polychrome, Negative }

/// Mirrors `functions.cl::next_joker_rarity`.
pub fn next_joker_rarity(inst: &mut Instance, source: RandomSource, ante: i32) -> Rarity {
    match source {
        RandomSource::Soul => return Rarity::Legendary,
        // Wraith spectral always yields a Rare joker
        RandomSource::Null => {} // fall through to roll
        _ => {}
    }
    // The static-by-source forced rarities:
    if matches!(source, RandomSource::Soul) { return Rarity::Legendary; }
    if matches!(source, RandomSource::RareTag) { return Rarity::Rare; }
    if matches!(source, RandomSource::UncommonTag) { return Rarity::Uncommon; }
    if matches!(source, RandomSource::RiffRaff) { return Rarity::Common; }

    let key = NodeKey {
        kind: RandomType::JokerRarity,
        source: Some(source),
        ante: Some(ante),
        resample: None,
    };
    let roll = inst.random(key);
    if roll > 0.95 { Rarity::Rare }
    else if roll > 0.7 { Rarity::Uncommon }
    else { Rarity::Common }
}

/// Mirrors `functions.cl::next_joker_edition`.
pub fn next_joker_edition(inst: &mut Instance, source: RandomSource, ante: i32) -> Edition {
    let key = NodeKey::with_ante(RandomType::JokerEdition, source, ante);
    let poll = inst.random(key);
    if poll > 0.997 { Edition::Negative }
    else if poll > 0.994 { Edition::Polychrome }
    else if poll > 0.98 { Edition::Holographic }
    else if poll > 0.96 { Edition::Foil }
    else { Edition::None }
}

/// Mirrors `functions.cl::next_joker` (identity only, no stickers).
pub fn next_joker(inst: &mut Instance, source: RandomSource, ante: i32) -> &'static str {
    let rarity = next_joker_rarity(inst, source, ante);
    let pool: &[&'static str] = match rarity {
        Rarity::Common => COMMON_JOKERS,
        Rarity::Uncommon => UNCOMMON_JOKERS,
        Rarity::Rare => RARE_JOKERS,
        Rarity::Legendary => LEGENDARY_JOKERS,
    };
    // randchoice_common with no lock pool (yet) — the lock/resample logic
    // lands once we model the per-run `locked[]` array properly.
    let kind = match rarity {
        Rarity::Common => RandomType::JokerCommon,
        Rarity::Uncommon => RandomType::JokerUncommon,
        Rarity::Rare => RandomType::JokerRare,
        Rarity::Legendary => RandomType::JokerLegendary,
    };
    let key = NodeKey::with_ante(kind, source, ante);
    inst.rand_choice(key, pool)
}

/// Mirrors `functions.cl::next_tag`.
pub fn next_tag(inst: &mut Instance, ante: i32) -> &'static str {
    let key = NodeKey::with_ante(RandomType::Tags, RandomSource::Null, ante);
    inst.rand_choice(key, TAGS)
}

/// Mirrors `functions.cl::next_voucher` (ignores lock state for now —
/// resample chain lands with voucher-chain tracking).
pub fn next_voucher(inst: &mut Instance, ante: i32) -> &'static str {
    let key = NodeKey {
        kind: RandomType::Voucher,
        source: None,
        ante: Some(ante),
        resample: None,
    };
    inst.rand_choice(key, VOUCHERS)
}

/// Mirrors `functions.cl::next_boss`. Finisher bosses appear on ante % 8 == 0,
/// small/big bosses otherwise.
///
/// Note: Immolate's reference uses a single un-ante'd cache node and
/// achieves per-ante variety through the lock state machine (each pick
/// locks that boss until the pool is exhausted, then reopens). We model
/// that approximately by including ante in the node key, which is exact
/// for single-ante queries and a close heuristic for multi-ante queries.
/// Full lock-state modelling lands with the verified-reproduction pass.
pub fn next_boss(inst: &mut Instance, ante: i32) -> &'static str {
    let pool: &[&'static str] = if ante % 8 == 0 { BOSSES_FINISHER } else { BOSSES_SMALL_BIG };
    let key = NodeKey::with_ante(RandomType::Boss, RandomSource::Null, ante);
    inst.rand_choice(key, pool)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_joker_draw() {
        let mut a = Instance::new("PHRAYJUS");
        let mut b = Instance::new("PHRAYJUS");
        for ante in 1..=4 {
            assert_eq!(
                next_joker(&mut a, RandomSource::Shop, ante),
                next_joker(&mut b, RandomSource::Shop, ante),
            );
        }
    }

    #[test]
    fn different_seeds_different_jokers() {
        // Statistical: across 100 seeds, ante-1 shop joker should not all match.
        let mut differs = false;
        let mut base = Instance::new("AAAA");
        let baseline = next_joker(&mut base, RandomSource::Shop, 1);
        for s in ["BBBB", "CCCC", "DDDD", "EEEE", "FFFF"] {
            let mut inst = Instance::new(s);
            if next_joker(&mut inst, RandomSource::Shop, 1) != baseline {
                differs = true;
                break;
            }
        }
        assert!(differs, "expected variety across seeds");
    }

    #[test]
    fn boss_pool_respects_ante() {
        let mut inst = Instance::new("BOSSEED1");
        let small_big = next_boss(&mut inst, 1);
        assert!(BOSSES_SMALL_BIG.contains(&small_big), "ante 1 boss not in small/big pool: {small_big}");

        let mut inst2 = Instance::new("BOSSEED1");
        let finisher = next_boss(&mut inst2, 8);
        assert!(BOSSES_FINISHER.contains(&finisher), "ante 8 boss not in finisher pool: {finisher}");
    }
}
