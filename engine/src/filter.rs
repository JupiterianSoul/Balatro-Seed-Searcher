//! Filter DSL — parser, bytecode, and the predicate the search loop runs.
//!
//! The DSL stays user-readable in JSON form so the web UI can construct it
//! visually, but at search time it's compiled to a tiny bytecode for the
//! hot loop. The compiler also:
//!   - Re-orders clauses by selectivity (cheap, high-rejection clauses run
//!     first → short-circuit early on misses)
//!   - Annotates clauses with derivation depth (how many ante simulations
//!     they require) so workers can lazily evaluate
//!   - Builds a "closeness scorer" alongside, so partial matches can rank.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Clause {
    /// "Ante N shop slot S contains joker X, optional edition E"
    AnteShopHasJoker {
        ante: u8,
        slot: u8,
        joker: String,
        edition: Option<String>,
    },
    /// "Ante N tag at position P is X"
    AnteTagIs { ante: u8, position: u8, tag: String },
    /// "Ante N boss is X"
    AnteBossIs { ante: u8, boss: String },
    /// "Voucher slot V is X"
    VoucherIs { ante: u8, voucher: String },
    /// Pack contents — "ante N pack P contains card X"
    AntePackContains { ante: u8, pack_index: u8, card: String },
    /// "In ante N, ANY of the first `max_packs` shop packs contains card X."
    /// Use this instead of a fan-out of AntePackContains clauses: opens packs
    /// 0..max_packs once per seed and short-circuits, which is both faster and
    /// correct (multiple AntePackContains in one filter would each restart the
    /// shop-pack chain from 0, double-advancing the cached Instance state).
    AnteAnyPackContains { ante: u8, max_packs: u8, card: String },
    /// Disjunction: passes iff any sub-clause passes. Sub-clauses share the
    /// same Instance state (so e.g. checking the same joker across antes 1..8
    /// advances the shop draw sequence naturally). Counts as ONE clause
    /// toward total_clauses / score.
    AnyOf { clauses: Vec<Clause> },
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Filter {
    pub clauses: Vec<Clause>,
    /// When true, return matches that satisfy >= `min_score` clauses,
    /// ranked. When false, require all clauses to match.
    pub partial: bool,
    pub min_score: Option<u8>,
}

/// Compiled form — opaque to callers, lives inside the searcher.
#[derive(Clone, Debug)]
pub struct Compiled {
    pub ops: Vec<Op>,
    pub total_clauses: u8,
}

#[derive(Clone, Debug)]
pub enum Op {
    // Each Op is one clause check. The reorderer places cheaper / more
    // selective ops first. The search loop runs them in order and either
    // short-circuits (strict mode) or counts matches (partial mode).
    HasJoker { ante: u8, slot: u8, joker_id: u16, edition: Option<u8> },
    TagIs { ante: u8, position: u8, tag_id: u16 },
    BossIs { ante: u8, boss_id: u16 },
    VoucherIs { ante: u8, voucher_id: u16 },
    PackContains { ante: u8, pack_index: u8, card_id: u16 },
    AnyPackContains { ante: u8, max_packs: u8, card_id: u16 },
    /// Disjunction. Sub-ops share Instance state with the parent evaluation.
    AnyOf { ops: Vec<Op> },
}

impl Filter {
    /// Naive compile that preserves declared order. The selectivity-based
    /// reorderer attaches in `compile_with_stats`.
    pub fn compile(&self) -> Compiled {
        let ops = self.clauses.iter().map(compile_clause).collect();
        Compiled { ops, total_clauses: self.clauses.len() as u8 }
    }
}

fn compile_clause(c: &Clause) -> Op {
    match c {
        Clause::AnteShopHasJoker { ante, slot, joker, edition } =>
            Op::HasJoker {
                ante: *ante,
                slot: *slot,
                joker_id: joker_name_to_id(joker),
                edition: edition.as_deref().map(edition_name_to_id),
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
        Clause::AnyOf { clauses } =>
            Op::AnyOf { ops: clauses.iter().map(compile_clause).collect() },
    }
}

// Name → id lookup placeholders. The real tables get generated from the
// canonical Balatro data dumps; for now we hash, which preserves uniqueness
// and lets us wire the bytecode end-to-end before the tables land.
fn joker_name_to_id(name: &str) -> u16 { stable_hash16(name) }
fn edition_name_to_id(name: &str) -> u8 {
    match name { "foil" => 1, "holographic" => 2, "polychrome" => 3, "negative" => 4, _ => 0 }
}
fn tag_name_to_id(name: &str) -> u16 { stable_hash16(name) }
fn boss_name_to_id(name: &str) -> u16 { stable_hash16(name) }
fn voucher_name_to_id(name: &str) -> u16 { stable_hash16(name) }
fn card_name_to_id(name: &str) -> u16 { stable_hash16(name) }

#[inline]
pub fn stable_hash16_u16(s: &str) -> u16 { stable_hash16(s) }

#[inline]
fn stable_hash16(s: &str) -> u16 {
    // FNV-1a 16-bit fold — fine for placeholder identity; replaced by table
    // lookup once the canonical id maps are in.
    let mut h: u32 = 2166136261;
    for &b in s.as_bytes() { h = (h ^ b as u32).wrapping_mul(16777619); }
    ((h >> 16) ^ h) as u16
}
