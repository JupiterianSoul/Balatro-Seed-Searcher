// Smoke test the AnyOf clause — search for "Blueprint in any of antes 1..8"
// over 200k seeds and confirm we get a reasonable hit rate.

use std::time::Instant;
use balatro_seed_engine::filter::{Clause, Filter};
use balatro_seed_engine::search::Searcher;
use balatro_seed_engine::state::{Deck, RunConfig, Stake};

fn run(label: &str, filter: Filter, count: u64, partial: bool, min_score: u8) {
    let compiled = filter.compile();
    let cfg = RunConfig { deck: Deck::Red, stake: Stake::White, seed: [0; 8], seed_len: 8 };
    let s = Searcher { config: &cfg, filter: &compiled, partial, min_score };
    let t = Instant::now();
    let mut hits = 0u64;
    let mut first: Option<String> = None;
    s.scan(0, count, |m| {
        hits += 1;
        if first.is_none() { first = Some(m.seed.clone()); }
    });
    let dt = t.elapsed();
    println!(
        "{:>40}  {} hits / {} seeds  ({:.2}%)  in {:.2?}  first={:?}",
        label, hits, count, hits as f64 / count as f64 * 100.0, dt, first,
    );
}

fn main() {
    // Single AND clause: Blueprint exactly in ante 1 shop slot 0
    run(
        "strict: Blueprint @ ante 1",
        Filter {
            clauses: vec![Clause::AnteShopHasJoker { ante: 1, slot: 0, joker: "Blueprint".into(), edition: None }],
            partial: false, min_score: None,
        },
        200_000, false, 0,
    );

    // AnyOf: Blueprint in any ante 1..8 shop slot 0
    run(
        "anyof: Blueprint in antes 1..8",
        Filter {
            clauses: vec![Clause::AnyOf {
                clauses: (1..=8).map(|a| Clause::AnteShopHasJoker {
                    ante: a, slot: 0, joker: "Blueprint".into(), edition: None,
                }).collect(),
            }],
            partial: false, min_score: None,
        },
        200_000, false, 0,
    );

    // Two AnyOf clauses (AND of ORs): Blueprint OR-of-antes 1..8 AND Brainstorm OR-of-antes 1..8
    run(
        "anyof x2 (AND of ORs)",
        Filter {
            clauses: vec![
                Clause::AnyOf {
                    clauses: (1..=8).map(|a| Clause::AnteShopHasJoker {
                        ante: a, slot: 0, joker: "Blueprint".into(), edition: None,
                    }).collect(),
                },
                Clause::AnyOf {
                    clauses: (1..=8).map(|a| Clause::AnteShopHasJoker {
                        ante: a, slot: 0, joker: "Brainstorm".into(), edition: None,
                    }).collect(),
                },
            ],
            partial: false, min_score: None,
        },
        500_000, false, 0,
    );
}
