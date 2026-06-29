//! The hot loop — scan a seed range, evaluate filter, emit matches.
//!
//! Strict mode short-circuits on the first failing clause; partial mode
//! counts matches and ranks by score. Filter clauses are pre-compiled into
//! bytecode (`filter::Op`) and run in selectivity order so the rarest
//! clauses fail-fast strict-AND searches.
//!
//! Performance design:
//!   - One `Instance::new(seed)` per seed (paying `pseudohash` + the
//!     LuaRandom 10-iter warmup once).
//!   - Each clause `clone()`s that template into a sub-Instance, which
//!     skips the warmup. Array-and-BTreeSet clone is ~5-10x cheaper than
//!     `Instance::new` for typical caches.
//!   - Clauses are re-ordered at compile time by estimated selectivity:
//!     rarest first means strict-AND searches reject seeds with the
//!     fewest cycles spent.

use crate::derive::{
    next_boss, next_pack, next_shop_item, next_tag, next_voucher,
    open_pack, open_pack_detailed, resolve_soul_legendary, resolve_wraith_rare,
    Edition, PackContents, ShopItemType, ShopSlot, StandardCard, Seal,
};
use crate::filter::{Compiled, Op};
use crate::instance::Instance;
use crate::seed::Seed;
use crate::state::RunConfig;

#[derive(Clone, Debug)]
pub struct Match {
    pub seed: String,
    pub rank: u64,
    pub score: u8,
}

pub struct Searcher<'a> {
    pub config: &'a RunConfig,
    pub filter: &'a Compiled,
    pub partial: bool,
    pub min_score: u8,
}

impl<'a> Searcher<'a> {
    /// Scan `[start_rank, start_rank + count)` and call `emit` for each hit.
    pub fn scan<F: FnMut(Match)>(&self, start_rank: u64, count: u64, mut emit: F) -> u64 {
        let mut seed = Seed::from_rank(start_rank, self.config.seed_len);
        let mut scanned: u64 = 0;

        while scanned < count {
            let mut buf = [0u8; 8];
            let len = seed.write_to(&mut buf);
            // SAFETY: write_to only emits ASCII alphabet bytes.
            let seed_str = unsafe { core::str::from_utf8_unchecked(&buf[..len]) };

            let score = self.evaluate(seed_str);
            let pass = if self.partial { score >= self.min_score }
                       else { score == self.filter.total_clauses };

            if pass {
                emit(Match {
                    seed: seed_str.to_string(),
                    rank: start_rank + scanned,
                    score,
                });
            }

            seed.increment();
            scanned += 1;
        }
        scanned
    }

    /// Evaluate compiled filter against a single seed.
    ///
    /// Builds ONE template Instance per seed (where `pseudohash` and the
    /// LuaRandom 10-iter warmup happen), then `clone()`s it for each clause
    /// that needs an isolated sub-run.
    #[inline]
    fn evaluate(&self, seed_str: &str) -> u8 {
        let mut template = Instance::new(seed_str);
        template.deck = self.config.deck;
        template.stake = self.config.stake;
        let mut score: u8 = 0;

        for op in &self.filter.ops {
            let hit = eval_op(&template, op);
            if hit { score += 1; }
            else if !self.partial { return score; }
        }
        score
    }
}

/// Evaluate a single op against a TEMPLATE Instance (immutable reference).
///
/// Every variant that needs to mutate engine state clones the template
/// into a `sub` Instance. The template never mutates, so clauses are
/// order-independent and the compiler can sort them by selectivity.
fn eval_op(inst: &Instance, op: &Op) -> bool {
    match op {
        Op::HasJoker { ante, slot, joker_id, edition, sticker } => {
            // Multi-slot shop scan. `slot=255` is the sentinel for
            // "any of the first 16 slots" — covers the default 4-slot
            // shop plus up to 12 rerolls.
            let scan_end = if *slot == 255 { 16 } else { (*slot as usize) + 1 };
            let scan_target_only = *slot != 255;
            let mut sub = inst.clone();
            for s in 0..scan_end {
                let slot_data = next_shop_item(&mut sub, *ante as i32);
                let is_target = if scan_target_only { s + 1 == scan_end } else { true };
                if !is_target { continue; }
                if matches_shop_slot(&slot_data, *joker_id, *edition, *sticker) {
                    return true;
                }
            }
            false
        }
        Op::TagIs { ante, position, tag_id } => {
            // position 0 = small-blind tag, position 1 = big-blind tag.
            // Mirrors Immolate: two consecutive `next_tag` draws per ante.
            let mut sub = inst.clone();
            let p = *position as usize;
            let mut drawn: &'static str = "";
            for i in 0..=p {
                drawn = next_tag(&mut sub, *ante as i32);
                let _ = i;
            }
            crate::filter::stable_hash16_u16(drawn) == *tag_id
        }
        Op::BossIs { ante, boss_id } => {
            let mut sub = inst.clone();
            let drawn = next_boss(&mut sub, *ante as i32);
            crate::filter::stable_hash16_u16(drawn) == *boss_id
        }
        Op::VoucherIs { ante, voucher_id } => {
            let mut sub = inst.clone();
            let drawn = next_voucher(&mut sub, *ante as i32);
            crate::filter::stable_hash16_u16(drawn) == *voucher_id
        }
        Op::PackContains { ante, pack_index, card_id } => {
            let mut sub = inst.clone();
            let want = *card_id;
            let mut last_pack: &'static str = "";
            for _ in 0..=*pack_index {
                last_pack = next_pack(&mut sub, *ante as i32);
            }
            let contents = open_pack_detailed(&mut sub, last_pack, *ante as i32);
            pack_contains_id(&contents, want)
        }
        Op::AnyPackContains { ante, max_packs, card_id } => {
            let mut sub = inst.clone();
            let want = *card_id;
            let mut found = false;
            for _ in 0..*max_packs {
                let pack = next_pack(&mut sub, *ante as i32);
                let contents = open_pack_detailed(&mut sub, pack, *ante as i32);
                if pack_contains_id(&contents, want) { found = true; break; }
            }
            found
        }
        Op::SoulIs { ante, max_packs, joker_id } => {
            let mut sub = inst.clone();
            let want = *joker_id;
            let mut found = false;
            for _ in 0..*max_packs {
                let pack = next_pack(&mut sub, *ante as i32);
                let contents = open_pack(&mut sub, pack, *ante as i32);
                if pack_contains_id(&contents, crate::filter::stable_hash16_u16("The Soul")) {
                    let leg = resolve_soul_legendary(&mut sub, *ante as i32);
                    if crate::filter::stable_hash16_u16(leg) == want {
                        found = true;
                        break;
                    }
                }
            }
            found
        }
        Op::WraithIs { ante, max_packs, joker_id } => {
            let mut sub = inst.clone();
            let want = *joker_id;
            let mut found = false;
            for _ in 0..*max_packs {
                let pack = next_pack(&mut sub, *ante as i32);
                let contents = open_pack(&mut sub, pack, *ante as i32);
                if pack_contains_id(&contents, crate::filter::stable_hash16_u16("Wraith")) {
                    let rare = resolve_wraith_rare(&mut sub, *ante as i32);
                    if crate::filter::stable_hash16_u16(rare) == want {
                        found = true;
                        break;
                    }
                }
            }
            found
        }
        Op::StandardCardIs { ante, max_packs, base_id, enhancement, edition, seal } => {
            let mut sub = inst.clone();
            let mut found = false;
            for _ in 0..*max_packs {
                let pack = next_pack(&mut sub, *ante as i32);
                let detailed = open_pack_detailed(&mut sub, pack, *ante as i32);
                if let PackContents::StandardCards(cards) = &detailed {
                    for c in cards {
                        if matches_standard_card(c, *base_id, *enhancement, *edition, *seal) {
                            found = true; break;
                        }
                    }
                    if found { break; }
                }
            }
            found
        }
        Op::AnyOf { ops } => {
            for sub in ops {
                if eval_op(inst, sub) { return true; }
            }
            false
        }
    }
}

#[inline]
fn pack_contains_id(c: &PackContents, want: u16) -> bool {
    let items: &[&'static str] = match c {
        PackContents::Tarots(v) | PackContents::Planets(v) |
        PackContents::Spectrals(v) | PackContents::Jokers(v) => v.as_slice(),
        PackContents::StandardCards(cards) => {
            for c in cards {
                if crate::filter::stable_hash16_u16(c.base) == want { return true; }
            }
            return false;
        }
        PackContents::Standard | PackContents::Unknown => &[],
    };
    items.iter().any(|it| crate::filter::stable_hash16_u16(it) == want)
}

#[inline]
fn matches_shop_slot(
    slot: &ShopSlot,
    joker_id: u16,
    want_edition: Option<u8>,
    want_sticker: Option<u8>,
) -> bool {
    if !matches!(slot.kind, ShopItemType::Joker) { return false; }
    if crate::filter::stable_hash16_u16(slot.item) != joker_id { return false; }
    if let Some(e) = want_edition {
        if edition_idx(slot.edition) != e { return false; }
    }
    if let Some(s) = want_sticker {
        let ok = match s {
            1 => slot.stickers.eternal,
            2 => slot.stickers.perishable,
            3 => slot.stickers.rental,
            _ => true,
        };
        if !ok { return false; }
    }
    true
}

#[inline]
fn matches_standard_card(
    c: &StandardCard,
    base_id: u16,
    want_enh: Option<u16>,
    want_edition: Option<u8>,
    want_seal: Option<u8>,
) -> bool {
    if base_id != 0 && crate::filter::stable_hash16_u16(c.base) != base_id { return false; }
    if let Some(eid) = want_enh {
        match c.enhancement {
            Some(e) if crate::filter::stable_hash16_u16(e) == eid => {}
            _ => return false,
        }
    }
    if let Some(e) = want_edition {
        if edition_idx(c.edition) != e { return false; }
    }
    if let Some(s) = want_seal {
        if seal_idx(c.seal) != s { return false; }
    }
    true
}

#[inline]
fn edition_idx(e: Edition) -> u8 {
    match e {
        Edition::None => 0, Edition::Foil => 1, Edition::Holographic => 2,
        Edition::Polychrome => 3, Edition::Negative => 4,
    }
}
#[inline]
fn seal_idx(s: Seal) -> u8 {
    match s {
        Seal::None => 0, Seal::Red => 1, Seal::Blue => 2,
        Seal::Gold => 3, Seal::Purple => 4,
    }
}
