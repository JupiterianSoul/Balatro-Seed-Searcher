// Throughput probe. Measures seeds/sec the native engine can scan under
// a few representative filter shapes — gives us a ceiling for what
// WASM/WASM-SIMD/browser will need to approach.

use std::time::Instant;
use balatro_seed_engine::filter::{Clause, Filter};
use balatro_seed_engine::search::Searcher;
use balatro_seed_engine::state::{Deck, RunConfig, Stake};

fn bench(label: &str, clauses: Vec<Clause>, count: u64) {
    let filter = Filter { clauses, partial: false, min_score: None };
    let compiled = filter.compile();
    let cfg = RunConfig { deck: Deck::Red, stake: Stake::White, seed: [0; 8], seed_len: 8 };
    let s = Searcher { config: &cfg, filter: &compiled, partial: false, min_score: 0 };

    let t = Instant::now();
    let mut hits = 0u64;
    s.scan(0, count, |_| hits += 1);
    let dt = t.elapsed();
    let rate = count as f64 / dt.as_secs_f64();
    println!(
        "{:>32}  {:>10} seeds  {:>8.2?}  {:>8.0}k/s  ({hits} hits)",
        label,
        count.to_string(),
        dt,
        rate / 1000.0,
    );
}

fn main() {
    println!("{:>32}  {:>10}        {:>8}  {:>8}", "filter", "count", "time", "rate");
    println!("{}", "-".repeat(80));

    bench(
        "1-clause joker ante1",
        vec![Clause::AnteShopHasJoker { ante: 1, slot: 0, joker: "Blueprint".into(), edition: None }],
        2_000_000,
    );

    bench(
        "2-clause joker+boss",
        vec![
            Clause::AnteShopHasJoker { ante: 1, slot: 0, joker: "Blueprint".into(), edition: None },
            Clause::AnteBossIs { ante: 3, boss: "TheHook".into() },
        ],
        2_000_000,
    );

    bench(
        "joker any-slot antes 1..8",
        (1..=8).map(|a| Clause::AnteShopHasJoker { ante: a, slot: 0, joker: "Blueprint".into(), edition: None }).collect(),
        500_000,
    );

    bench(
        "voucher ante1",
        vec![Clause::VoucherIs { ante: 1, voucher: "Overstock".into() }],
        2_000_000,
    );

    bench(
        "tag pos0 ante1",
        vec![Clause::AnteTagIs { ante: 1, position: 0, tag: "RareTag".into() }],
        2_000_000,
    );

    bench(
        "pack arcana ante1 contains The Fool",
        vec![Clause::AntePackContains { ante: 1, pack_index: 0, card: "The Fool".into() }],
        500_000,
    );

    bench(
        "5-clause mixed (joker+voucher+tag+boss+pack)",
        vec![
            Clause::AnteShopHasJoker { ante: 1, slot: 0, joker: "Blueprint".into(), edition: None },
            Clause::VoucherIs { ante: 1, voucher: "Overstock".into() },
            Clause::AnteTagIs { ante: 1, position: 0, tag: "RareTag".into() },
            Clause::AnteBossIs { ante: 3, boss: "TheHook".into() },
            Clause::AntePackContains { ante: 2, pack_index: 0, card: "The Fool".into() },
        ],
        500_000,
    );

    bench(
        "16-clause joker x16 antes",
        (1..=8).flat_map(|a| {
            ["Blueprint", "Brainstorm"].into_iter().map(move |j|
                Clause::AnteShopHasJoker { ante: a, slot: 0, joker: j.into(), edition: None })
        }).collect(),
        500_000,
    );
}
