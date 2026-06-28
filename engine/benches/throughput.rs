// Quick-and-dirty throughput probe. Counts seeds/sec the native engine
// can scan with the placeholder filter — gives us a ceiling for what
// WASM/WASM-SIMD/cloud will need to approach.

use std::time::Instant;
use balatro_seed_engine::filter::{Clause, Filter};
use balatro_seed_engine::search::Searcher;
use balatro_seed_engine::state::{Deck, RunConfig, Stake};

fn main() {
    let filter = Filter {
        clauses: vec![
            Clause::AnteShopHasJoker { ante: 1, slot: 0, joker: "Blueprint".into(), edition: None },
            Clause::AnteBossIs { ante: 3, boss: "TheHook".into() },
        ],
        partial: false,
        min_score: None,
    };
    let compiled = filter.compile();
    let cfg = RunConfig { deck: Deck::Red, stake: Stake::White, seed: [0; 8], seed_len: 8 };
    let s = Searcher { config: &cfg, filter: &compiled, partial: false, min_score: 0 };

    let count: u64 = 1_000_000;
    let t = Instant::now();
    let mut hits = 0;
    s.scan(0, count, |_| hits += 1);
    let dt = t.elapsed();

    println!("scanned {count} seeds in {dt:.2?} ({:.1}k seeds/sec native)",
        count as f64 / dt.as_secs_f64() / 1000.0);
    println!("{hits} matches with placeholder filter");
}
