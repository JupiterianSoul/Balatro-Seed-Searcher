//! The hot loop — scan a seed range, evaluate filter, emit matches.
//!
//! Strict mode short-circuits on the first failing clause; partial mode
//! counts matches and ranks by score. Filter clauses are pre-compiled into
//! bytecode (`filter::Op`) and run in selectivity order.

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
    #[inline]
    fn evaluate(&self, seed_str: &str) -> u8 {
        let mut inst = Instance::new(seed_str);
        inst.deck = self.config.deck;
        inst.stake = self.config.stake;
        let mut score: u8 = 0;

        for op in &self.filter.ops {
            let hit = eval_op(&mut inst, op);
            if hit { score += 1; }
            else if !self.partial { return score; }
        }
        score
    }
}

/// Evaluate a single op against an Instance. Recursive so AnyOf can call back
/// into eval_op for its children, all sharing the same Instance state.
fn eval_op(inst: &mut Instance, op: &Op) -> bool {
    match op {
        Op::HasJoker { ante, slot, joker_id, edition, sticker } => {
            // Multi-slot shop scan: simulate slots [0, slot] on a fresh sub-Instance
            // so the parent's RNG node cache stays clean. `slot` is u8, so we step
            // through (slot + 1) items and inspect the last one.
            // For slot = u8::MAX we treat it as "any slot in [0, 15)" — the common
            // "first N rerolls" use case.
            let scan_end = if *slot == 255 { 16 } else { (*slot as usize) + 1 };
            let scan_target_only = *slot != 255;
            let mut sub = Instance::new(inst.seed_str());
            sub.deck = inst.deck;
            sub.stake = inst.stake;
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
            let mut sub = Instance::new(inst.seed_str());
            sub.deck = inst.deck;
            sub.stake = inst.stake;
            let p = *position as usize;
            let mut drawn: &'static str = "";
            for i in 0..=p {
                drawn = next_tag(&mut sub, *ante as i32);
                let _ = i;
            }
            crate::filter::stable_hash16_u16(drawn) == *tag_id
        }
        Op::BossIs { ante, boss_id } => {
            let drawn = next_boss(inst, *ante as i32);
            crate::filter::stable_hash16_u16(drawn) == *boss_id
        }
        Op::VoucherIs { ante, voucher_id } => {
            let drawn = next_voucher(inst, *ante as i32);
            crate::filter::stable_hash16_u16(drawn) == *voucher_id
        }
        Op::PackContains { ante, pack_index, card_id } => {
            let mut sub = Instance::new(inst.seed_str());
            sub.deck = inst.deck;
            sub.stake = inst.stake;
            let want = *card_id;
            let mut found = false;
            let mut last_pack: &'static str = "";
            for _ in 0..=*pack_index {
                last_pack = next_pack(&mut sub, *ante as i32);
            }
            let contents = open_pack_detailed(&mut sub, last_pack, *ante as i32);
            found = pack_contains_id(&contents, want);
            found
        }
        Op::AnyPackContains { ante, max_packs, card_id } => {
            let mut sub = Instance::new(inst.seed_str());
            sub.deck = inst.deck;
            sub.stake = inst.stake;
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
            // Walk first `max_packs` shop packs of `ante`. For arcana / spectral
            // packs that contain The Soul, resolve which Legendary it forces.
            let mut sub = Instance::new(inst.seed_str());
            sub.deck = inst.deck;
            sub.stake = inst.stake;
            let want = *joker_id;
            let mut found = false;
            for _ in 0..*max_packs {
                let pack = next_pack(&mut sub, *ante as i32);
                let contents = open_pack(&mut sub, pack, *ante as i32);
                if pack_contains_id(&contents, crate::filter::stable_hash16_u16("The Soul")) {
                    // Soul appeared → resolve the legendary it forces.
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
            // Walk first `max_packs` shop packs. Spectral packs containing Wraith
            // → resolve which Rare joker Wraith forces.
            let mut sub = Instance::new(inst.seed_str());
            sub.deck = inst.deck;
            sub.stake = inst.stake;
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
            // Walk first `max_packs` shop packs. For Standard packs, simulate
            // each card and match against the requested constraints.
            let mut sub = Instance::new(inst.seed_str());
            sub.deck = inst.deck;
            sub.stake = inst.stake;
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
