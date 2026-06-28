//! The hot loop — scan a seed range, evaluate filter, emit matches.
//!
//! Strict mode short-circuits on the first failing clause; partial mode
//! counts matches and ranks by score. Filter clauses are pre-compiled into
//! bytecode (`filter::Op`) and run in selectivity order.

use crate::derive::{
    next_boss, next_joker, next_joker_edition, next_pack, next_tag, next_voucher,
    open_pack, Edition, PackContents,
};
use crate::filter::{Compiled, Op};
use crate::instance::{Instance, RandomSource};
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
        let mut score: u8 = 0;

        for op in &self.filter.ops {
            let hit = match op {
                Op::HasJoker { ante, slot: _, joker_id, edition } => {
                    // First slot only for now — multi-slot shop modelling lands
                    // with the full voucher chain. For the slot-0 case this is exact.
                    let drawn = next_joker(&mut inst, RandomSource::Shop, *ante as i32);
                    let name_match = crate::filter::stable_hash16_u16(drawn) == *joker_id;
                    let edition_match = match edition {
                        None => true,
                        Some(want) => {
                            let drawn_edition = next_joker_edition(&mut inst, RandomSource::Shop, *ante as i32);
                            edition_idx(drawn_edition) == *want
                        }
                    };
                    name_match && edition_match
                }
                Op::TagIs { ante, position: _, tag_id } => {
                    let drawn = next_tag(&mut inst, *ante as i32);
                    crate::filter::stable_hash16_u16(drawn) == *tag_id
                }
                Op::BossIs { ante, boss_id } => {
                    let drawn = next_boss(&mut inst, *ante as i32);
                    crate::filter::stable_hash16_u16(drawn) == *boss_id
                }
                Op::VoucherIs { ante, voucher_id } => {
                    let drawn = next_voucher(&mut inst, *ante as i32);
                    crate::filter::stable_hash16_u16(drawn) == *voucher_id
                }
                Op::PackContains { ante, pack_index, card_id } => {
                    // Open the (ante, pack_index)-th pack in shop ante `ante`
                    // and check whether the requested card appears inside.
                    // pack_index=0 → first pack of the shop.
                    let want = *card_id;
                    let mut found = false;
                    // Draw `pack_index + 1` packs so we land on the requested slot
                    let mut last_pack: &'static str = "";
                    for _ in 0..=*pack_index {
                        last_pack = next_pack(&mut inst, *ante as i32);
                    }
                    let contents = open_pack(&mut inst, last_pack, *ante as i32);
                    let items: &[&'static str] = match &contents {
                        PackContents::Tarots(v) | PackContents::Planets(v) |
                        PackContents::Spectrals(v) | PackContents::Jokers(v) => v.as_slice(),
                        PackContents::Standard | PackContents::Unknown => &[],
                    };
                    for it in items {
                        if crate::filter::stable_hash16_u16(it) == want {
                            found = true;
                            break;
                        }
                    }
                    found
                }
            };
            if hit { score += 1; }
            else if !self.partial { return score; }
        }
        score
    }
}

#[inline]
fn edition_idx(e: Edition) -> u8 {
    match e {
        Edition::None => 0, Edition::Foil => 1, Edition::Holographic => 2,
        Edition::Polychrome => 3, Edition::Negative => 4,
    }
}
