//! Filter DSL — parser, bytecode, and the predicate the search loop runs.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Clause {
    /// "Ante N shop slot S contains joker X, optional edition E, optional sticker"
    /// `slot=255` (sentinel) → ANY of the first 16 slots (covers default 4-slot shop
    /// + up to 12 rerolls without overshooting).
    AnteShopHasJoker {
        ante: u8,
        slot: u8,
        joker: String,
        edition: Option<String>,
        sticker: Option<String>,
    },
    /// "Ante N tag at position P is X". position 0 = small-blind tag, 1 = big-blind.
    AnteTagIs { ante: u8, position: u8, tag: String },
    /// "Ante N boss is X"
    AnteBossIs { ante: u8, boss: String },
    /// "Voucher slot V is X"
    VoucherIs { ante: u8, voucher: String },
    /// Pack contents — "ante N pack P contains card X"
    AntePackContains { ante: u8, pack_index: u8, card: String },
    /// "Ante N, ANY of the first `max_packs` shop packs contains card X."
    AnteAnyPackContains { ante: u8, max_packs: u8, card: String },
    /// Legendary joker via Soul resolution.
    /// "Ante N, any Arcana/Spectral pack in the first `max_packs` shop packs
    /// contains The Soul whose forced legendary is `joker`."
    AnteSoulIs { ante: u8, max_packs: u8, joker: String },
    /// Rare joker via Wraith resolution.
    /// "Ante N, any Spectral pack in the first `max_packs` shop packs contains
    /// Wraith whose forced Rare joker is `joker`."
    AnteWraithIs { ante: u8, max_packs: u8, joker: String },
    /// Standard-pack card constraints. `base` empty = any rank/suit; same for
    /// the other Option fields. Walks first `max_packs` shop packs and checks
    /// every Standard-pack card against the constraints.
    AnteStandardCardIs {
        ante: u8,
        max_packs: u8,
        base: String,
        enhancement: Option<String>,
        edition: Option<String>,
        seal: Option<String>,
    },
    /// Disjunction.
    AnyOf { clauses: Vec<Clause> },
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Filter {
    pub clauses: Vec<Clause>,
    /// When true, return matches that satisfy >= `min_score` clauses, ranked.
    pub partial: bool,
    pub min_score: Option<u8>,
}

#[derive(Clone, Debug)]
pub struct Compiled {
    pub ops: Vec<Op>,
    pub total_clauses: u8,
}

#[derive(Clone, Debug)]
pub enum Op {
    HasJoker { ante: u8, slot: u8, joker_id: u16, edition: Option<u8>, sticker: Option<u8> },
    TagIs { ante: u8, position: u8, tag_id: u16 },
    BossIs { ante: u8, boss_id: u16 },
    VoucherIs { ante: u8, voucher_id: u16 },
    PackContains { ante: u8, pack_index: u8, card_id: u16 },
    AnyPackContains { ante: u8, max_packs: u8, card_id: u16 },
    SoulIs { ante: u8, max_packs: u8, joker_id: u16 },
    WraithIs { ante: u8, max_packs: u8, joker_id: u16 },
    StandardCardIs {
        ante: u8,
        max_packs: u8,
        base_id: u16,
        enhancement: Option<u16>,
        edition: Option<u8>,
        seal: Option<u8>,
    },
    AnyOf { ops: Vec<Op> },
}

impl Filter {
    pub fn compile(&self) -> Compiled {
        let mut ops: Vec<Op> = self.clauses.iter().map(compile_clause).collect();
        // Selectivity-ordered execution: in strict-AND mode (the default),
        // `Searcher::evaluate` short-circuits on the first failing clause.
        // Putting the rarest clauses first means most seeds are rejected
        // by the cheapest-to-fail probe. In partial mode we still score
        // every clause, so ordering doesn't affect correctness; it only
        // affects branch-prediction patterns. We sort unconditionally
        // because clauses are pure and order-independent (each clause
        // clones the template Instance before mutating).
        ops.sort_by(|a, b| {
            // Lower selectivity = rarer = check first.
            estimated_selectivity(a)
                .partial_cmp(&estimated_selectivity(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Compiled { ops, total_clauses: self.clauses.len() as u8 }
    }
}

/// Rough analytical priors per clause family. Sourced from PARITY.md
/// smoke runs. These are LOG-scale estimates; we only need a stable
/// ordering, not calibrated probabilities. Lower = rarer = run first.
fn estimated_selectivity(op: &Op) -> f64 {
    match op {
        // Edition-constrained shop slot is the rarest single probe.
        Op::HasJoker { edition: Some(_), .. } => 0.0003,
        // Sticker-only is mid-rare (stake-gated).
        Op::HasJoker { sticker: Some(_), .. } => 0.01,
        // Wraith→specific rare: pack-rate × rare-rate × 1/60 ≈ 6e-4.
        Op::WraithIs { .. } => 0.0006,
        // Soul→specific legendary: 1/14 per Soul × ~10% Soul-hit rate.
        Op::SoulIs { .. } => 0.007,
        // Plain shop joker over any of 16 slots: ~3% per pool.
        Op::HasJoker { .. } => 0.03,
        // Specific voucher per ante.
        Op::VoucherIs { .. } => 0.03,
        // Standard card with edition: very rare.
        Op::StandardCardIs { edition: Some(_), .. } => 0.0003,
        // Standard card with seal but no edition.
        Op::StandardCardIs { seal: Some(_), .. } => 0.04,
        // Plain standard card (base only).
        Op::StandardCardIs { .. } => 0.07,
        // Tag at a specific position.
        Op::TagIs { .. } => 0.04,
        // Boss is 1 of ~18 valid bosses at the ante.
        Op::BossIs { .. } => 0.06,
        // Pack content probes — single ante.
        Op::PackContains { .. } => 0.08,
        Op::AnyPackContains { .. } => 0.10,
        // AnyOf is as selective as its tightest child (union scan, but
        // short-circuits on first hit). Use the MIN child selectivity
        // for ordering vs siblings, but bump up because we still pay
        // for the children that miss before the hit.
        Op::AnyOf { ops } => {
            ops.iter()
                .map(estimated_selectivity)
                .fold(1.0_f64, f64::min)
                * (ops.len() as f64).max(1.0)
        }
    }
}

fn compile_clause(c: &Clause) -> Op {
    match c {
        Clause::AnteShopHasJoker { ante, slot, joker, edition, sticker } =>
            Op::HasJoker {
                ante: *ante,
                slot: *slot,
                joker_id: joker_name_to_id(joker),
                edition: edition.as_deref().map(edition_name_to_id),
                sticker: sticker.as_deref().map(sticker_name_to_id),
            },
        Clause::AnteTagIs { ante, position, tag } =>
            Op::TagIs { ante: *ante, position: *position, tag_id: tag_name_to_id(tag) },
        Clause::AnteBossIs { ante, boss } =>
            Op::BossIs { ante: *ante, boss_id: boss_name_to_id(boss) },
        Clause::VoucherIs { ante, voucher } =>
            Op::VoucherIs { ante: *ante, voucher_id: voucher_name_to_id(voucher) },
        Clause::AntePackContains { ante, pack_index, card } =>
            Op::PackContains { ante: *ante, pack_index: *pack_index, card_id: card_name_to_id(card) },
        Clause::AnteAnyPackContains { ante, max_packs, card } =>
            Op::AnyPackContains { ante: *ante, max_packs: *max_packs, card_id: card_name_to_id(card) },
        Clause::AnteSoulIs { ante, max_packs, joker } =>
            Op::SoulIs { ante: *ante, max_packs: *max_packs, joker_id: joker_name_to_id(joker) },
        Clause::AnteWraithIs { ante, max_packs, joker } =>
            Op::WraithIs { ante: *ante, max_packs: *max_packs, joker_id: joker_name_to_id(joker) },
        Clause::AnteStandardCardIs { ante, max_packs, base, enhancement, edition, seal } =>
            Op::StandardCardIs {
                ante: *ante,
                max_packs: *max_packs,
                base_id: if base.is_empty() { 0 } else { card_name_to_id(base) },
                enhancement: enhancement.as_deref().map(card_name_to_id),
                edition: edition.as_deref().map(edition_name_to_id),
                seal: seal.as_deref().map(seal_name_to_id),
            },
        Clause::AnyOf { clauses } =>
            Op::AnyOf { ops: clauses.iter().map(compile_clause).collect() },
    }
}

fn joker_name_to_id(name: &str) -> u16 { stable_hash16(name) }
fn edition_name_to_id(name: &str) -> u8 {
    match name { "foil" => 1, "holographic" => 2, "polychrome" => 3, "negative" => 4, _ => 0 }
}
fn sticker_name_to_id(name: &str) -> u8 {
    match name { "eternal" => 1, "perishable" => 2, "rental" => 3, _ => 0 }
}
fn seal_name_to_id(name: &str) -> u8 {
    match name {
        "red" | "red seal" | "Red Seal" => 1,
        "blue" | "blue seal" | "Blue Seal" => 2,
        "gold" | "gold seal" | "Gold Seal" => 3,
        "purple" | "purple seal" | "Purple Seal" => 4,
        _ => 0,
    }
}
fn tag_name_to_id(name: &str) -> u16 { stable_hash16(name) }
fn boss_name_to_id(name: &str) -> u16 { stable_hash16(name) }
fn voucher_name_to_id(name: &str) -> u16 { stable_hash16(name) }
fn card_name_to_id(name: &str) -> u16 { stable_hash16(name) }

#[inline]
pub fn stable_hash16_u16(s: &str) -> u16 { stable_hash16(s) }

#[inline]
fn stable_hash16(s: &str) -> u16 {
    let mut h: u32 = 2166136261;
    for &b in s.as_bytes() { h = (h ^ b as u32).wrapping_mul(16777619); }
    ((h >> 16) ^ h) as u16
}
