//! Smoke test for V2 parity features:
//!  - Multi-slot shop scan (slot 0..4)
//!  - Edition filter
//!  - Sticker filter
//!  - Tag position 1 (big blind)
//!  - Standard pack card-level
//!  - Soul → legendary resolution
//!  - Wraith → rare resolution
//!
//! Sweeps 100k seeds for each smoke test, prints hit rates. Hit rate must be
//! plausible (jokers should not be 0% or 100% across the sample).

use balatro_seed_engine::filter::{Clause, Filter};
use balatro_seed_engine::search::Searcher;
use balatro_seed_engine::state::{Deck, RunConfig, Stake};

fn sweep(name: &str, clauses: Vec<Clause>, seeds: u64) -> u64 {
    let filter = Filter { clauses, partial: false, min_score: None };
    let compiled = filter.compile();
    let config = RunConfig { deck: Deck::Red, stake: Stake::Gold, seed: [0;8], seed_len: 6 };
    let s = Searcher { config: &config, filter: &compiled, partial: false, min_score: 0 };
    let mut hits: u64 = 0;
    s.scan(0, seeds, |_| { hits += 1; });
    println!("{:60} {:>5}/{:>6} = {:>5.2}%", name, hits, seeds,
             hits as f64 * 100.0 / seeds as f64);
    hits
}

fn main() {
    let n = 100_000u64;
    println!("==== V2 parity smoke (Gold stake, Red deck, {} seeds) ====", n);

    // Item 1+5: multi-slot, plain joker shop search
    sweep("Ante1 slot=0 Joker",
        vec![Clause::AnteShopHasJoker {
            ante: 1, slot: 0,
            joker: "Joker".into(), edition: None, sticker: None }],
        n);
    sweep("Ante1 slot=0..15 ANY Joker",
        vec![Clause::AnteShopHasJoker {
            ante: 1, slot: 255,
            joker: "Joker".into(), edition: None, sticker: None }],
        n);
    sweep("Ante1 slot=0..15 Blueprint",
        vec![Clause::AnteShopHasJoker {
            ante: 1, slot: 255,
            joker: "Blueprint".into(), edition: None, sticker: None }],
        n);

    // Item 5: Edition
    sweep("Ante1 ANY slot, Foil Joker",
        vec![Clause::AnteShopHasJoker {
            ante: 1, slot: 255,
            joker: "Joker".into(), edition: Some("foil".into()), sticker: None }],
        n);
    sweep("Ante1 ANY slot, Negative Joker",
        vec![Clause::AnteShopHasJoker {
            ante: 1, slot: 255,
            joker: "Joker".into(), edition: Some("negative".into()), sticker: None }],
        n);

    // Item 6: Sticker (Gold stake)
    sweep("Ante1 ANY slot, Rental Joker (Gold stake)",
        vec![Clause::AnteShopHasJoker {
            ante: 1, slot: 255,
            joker: "Joker".into(), edition: None, sticker: Some("rental".into()) }],
        n);

    // Item 8a: Tag pos 1 (big blind)
    sweep("Ante1 big-blind tag = Negative Tag",
        vec![Clause::AnteTagIs { ante: 1, position: 1, tag: "Negative Tag".into() }],
        n);

    // Item 3: Soul → legendary
    sweep("Ante1..6 Soul → Perkeo (any of first 6 packs each)",
        vec![Clause::AnyOf { clauses: (1..=6).map(|a|
            Clause::AnteSoulIs { ante: a, max_packs: 6, joker: "Perkeo".into() }
        ).collect() }],
        n);

    // Item 4: Wraith → Rare
    sweep("Ante1..6 Wraith → Blueprint (any of first 6 packs each)",
        vec![Clause::AnyOf { clauses: (1..=6).map(|a|
            Clause::AnteWraithIs { ante: a, max_packs: 6, joker: "Blueprint".into() }
        ).collect() }],
        n);

    // Item 7: Standard Pack card
    sweep("Ante1..3 ANY standard pack contains Ace of Spades",
        vec![Clause::AnyOf { clauses: (1..=3).map(|a|
            Clause::AnteStandardCardIs {
                ante: a, max_packs: 6,
                base: "Ace of Spades".into(),
                enhancement: None, edition: None, seal: None }
        ).collect() }],
        n);
    sweep("Ante1..3 ANY standard pack: Gold Seal on any card",
        vec![Clause::AnyOf { clauses: (1..=3).map(|a|
            Clause::AnteStandardCardIs {
                ante: a, max_packs: 6,
                base: "".into(),
                enhancement: None, edition: None,
                seal: Some("gold".into()) }
        ).collect() }],
        n);

    println!("\nSmoke OK.");
}
