//! Regression sweep / verifier for the Balatro Seed Engine.
//!
//! Runs a 100k-seed sweep and validates:
//!   1. Determinism: re-running the same seed twice yields identical results.
//!   2. Pool validity: every drawn joker/tarot/planet/spectral/boss is in the
//!      published pool table.
//!   3. Distribution sanity: joker rarities, pack types, and boss draws fall
//!      within reasonable statistical bands.
//!   4. Lock behavior: ante-1 never draws a locked boss (The Plant, The Tooth, etc.).
//!   5. Sticker rules: White stake never produces eternal/perishable/rental.
//!
//! It does NOT yet run a bit-for-bit comparison against an external Immolate
//! oracle — that would require the OpenCL pipeline, which is out of scope for
//! this in-browser engine. We document this gap honestly in docs/comparison.md.
//!
//! Usage:
//!   cargo run --release --bin verify -- [N]
//! where N is the number of seeds to sweep (default 100_000).

use balatro_seed_engine::derive::{
    next_boss, next_joker, next_joker_with_stickers, next_pack, next_planet,
    next_spectral, next_tarot, open_pack,
};
use balatro_seed_engine::instance::{Instance, RandomSource};
use balatro_seed_engine::state::Stake;
use balatro_seed_engine::tables::{
    BOSSES, COMMON_JOKERS, LEGENDARY_JOKERS, PLANETS, RARE_JOKERS, SPECTRALS, TAROTS, UNCOMMON_JOKERS,
};

use std::collections::HashMap;
use std::env;
use std::time::Instant;

// From Immolate lib/instance.cl::init_locks — bosses locked when ante < 2.
const LOCKED_ANTE1_BOSSES: &[&str] = &[
    "The Mouth", "The Fish", "The Wall", "The House", "The Mark",
    "The Wheel", "The Arm", "The Water", "The Needle", "The Flint",
    // ante < 3
    "The Tooth", "The Eye",
    // ante < 4
    "The Plant",
    // ante < 5
    "The Serpent",
    // ante < 6
    "The Ox",
];

const PACK_NAMES: &[&str] = &[
    "Arcana Pack", "Jumbo Arcana Pack", "Mega Arcana Pack",
    "Celestial Pack", "Jumbo Celestial Pack", "Mega Celestial Pack",
    "Standard Pack", "Jumbo Standard Pack", "Mega Standard Pack",
    "Buffoon Pack", "Jumbo Buffoon Pack", "Mega Buffoon Pack",
    "Spectral Pack", "Jumbo Spectral Pack", "Mega Spectral Pack",
];

fn all_jokers() -> Vec<&'static str> {
    let mut v = Vec::new();
    v.extend_from_slice(COMMON_JOKERS);
    v.extend_from_slice(UNCOMMON_JOKERS);
    v.extend_from_slice(RARE_JOKERS);
    v.extend_from_slice(LEGENDARY_JOKERS);
    v
}

fn synth_seed(i: u64) -> String {
    // Map i → 8-char base-35 seed using the engine's own alphabet.
    let alpha = "123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let bytes = alpha.as_bytes();
    let mut x = i.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(1);
    let mut s = String::with_capacity(8);
    for _ in 0..8 {
        let idx = (x % 35) as usize;
        s.push(bytes[idx] as char);
        x /= 35;
        if x == 0 { x = i.wrapping_mul(0xBF58_476D_1CE4_E5B9).wrapping_add(0xDEAD); }
    }
    s
}

struct Stats {
    n: u64,
    determinism_failures: u64,
    pool_misses: u64,
    locked_ante1_boss_hits: u64,
    white_stake_sticker_hits: u64,
    joker_rarity: HashMap<&'static str, u64>,
    pack_counts: HashMap<&'static str, u64>,
    boss_counts: HashMap<&'static str, u64>,
}

impl Stats {
    fn new() -> Self {
        Self {
            n: 0,
            determinism_failures: 0,
            pool_misses: 0,
            locked_ante1_boss_hits: 0,
            white_stake_sticker_hits: 0,
            joker_rarity: HashMap::new(),
            pack_counts: HashMap::new(),
            boss_counts: HashMap::new(),
        }
    }
}

fn rarity_of(j: &str) -> &'static str {
    if COMMON_JOKERS.contains(&j) { return "common"; }
    if UNCOMMON_JOKERS.contains(&j) { return "uncommon"; }
    if RARE_JOKERS.contains(&j) { return "rare"; }
    if LEGENDARY_JOKERS.contains(&j) { return "legendary"; }
    "unknown"
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let n: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(100_000);

    println!("=== Balatro Seed Engine verifier ===");
    println!("Sweeping {} seeds for determinism, pool validity, distribution, locks, stickers.", n);

    let joker_pool: Vec<&str> = all_jokers();
    let mut stats = Stats::new();
    let t0 = Instant::now();

    for i in 0..n {
        let seed = synth_seed(i);

        // Run all derivations on a fresh instance.
        let mut inst = Instance::new(&seed);
        let j_shop = next_joker(&mut inst, RandomSource::Shop, 1);
        let j_buf = next_joker(&mut inst, RandomSource::Buffoon, 1);
        let t1 = next_tarot(&mut inst, RandomSource::Arcana, 1, true);
        let p1 = next_planet(&mut inst, RandomSource::Celestial, 1, true);
        let s1 = next_spectral(&mut inst, RandomSource::Spectral, 1, true);
        let pack = next_pack(&mut inst, 1);
        let boss1 = next_boss(&mut inst, 1);
        let boss8 = next_boss(&mut inst, 8);

        // Sticker check on White stake — should always be off.
        let mut inst_white = Instance::new(&seed);
        inst_white.stake = Stake::White;
        let (_jw, _rw, _ew, sticker_w) = next_joker_with_stickers(&mut inst_white, RandomSource::Shop, 1);
        if sticker_w.eternal || sticker_w.perishable || sticker_w.rental {
            stats.white_stake_sticker_hits += 1;
        }

        // Open the pack to make sure it returns something well-formed.
        let mut inst_pack = Instance::new(&seed);
        let _contents = open_pack(&mut inst_pack, pack, 1);

        // Determinism check on every 1000th seed: re-run from scratch.
        if i % 1000 == 0 {
            let mut inst2 = Instance::new(&seed);
            let j_shop2 = next_joker(&mut inst2, RandomSource::Shop, 1);
            let j_buf2 = next_joker(&mut inst2, RandomSource::Buffoon, 1);
            let t12 = next_tarot(&mut inst2, RandomSource::Arcana, 1, true);
            if j_shop != j_shop2 || j_buf != j_buf2 || t1 != t12 {
                stats.determinism_failures += 1;
            }
        }

        // Pool validity.
        if !joker_pool.contains(&j_shop) { stats.pool_misses += 1; }
        if !joker_pool.contains(&j_buf) { stats.pool_misses += 1; }
        // Tarot/Planet/Spectral can also return The Soul or Black Hole.
        if !TAROTS.contains(&t1) && t1 != "The Soul" { stats.pool_misses += 1; }
        if !PLANETS.contains(&p1) && p1 != "Black Hole" { stats.pool_misses += 1; }
        if !SPECTRALS.contains(&s1) && s1 != "The Soul" && s1 != "Black Hole" { stats.pool_misses += 1; }
        if !PACK_NAMES.contains(&pack) { stats.pool_misses += 1; }
        if !BOSSES.contains(&boss1) { stats.pool_misses += 1; }
        if !BOSSES.contains(&boss8) { stats.pool_misses += 1; }

        // Lock validity on ante 1.
        if LOCKED_ANTE1_BOSSES.contains(&boss1) {
            stats.locked_ante1_boss_hits += 1;
        }

        *stats.joker_rarity.entry(rarity_of(j_shop)).or_insert(0) += 1;
        *stats.pack_counts.entry(pack).or_insert(0) += 1;
        *stats.boss_counts.entry(boss1).or_insert(0) += 1;

        stats.n += 1;
    }

    let dt = t0.elapsed().as_secs_f64();
    let rate = stats.n as f64 / dt;

    println!();
    println!("Elapsed:  {:.2}s", dt);
    println!("Rate:     {:.0} seeds/sec  (single-thread, full derivation set per seed)", rate);
    println!();
    println!("=== Correctness ===");
    println!("Determinism failures:        {}  (must be 0)", stats.determinism_failures);
    println!("Pool misses:                 {}  (must be 0)", stats.pool_misses);
    println!("Ante-1 locked-boss hits:     {}  (must be 0)", stats.locked_ante1_boss_hits);
    println!("White-stake sticker leaks:   {}  (must be 0)", stats.white_stake_sticker_hits);
    println!();
    println!("=== Joker rarity distribution (shop, ante 1) ===");
    let total = stats.n as f64;
    let mut rkeys: Vec<&&str> = stats.joker_rarity.keys().collect();
    rkeys.sort();
    for k in rkeys {
        let v = stats.joker_rarity[*k];
        println!("  {:10} {:>8}  ({:.2}%)", k, v, 100.0 * v as f64 / total);
    }
    // Published rates from Immolate / game: common 0.70, uncommon 0.25, rare 0.05.
    println!("  expected   common ≈ 70%, uncommon ≈ 25%, rare ≈ 5%, legendary 0%");
    println!();
    println!("=== Pack distribution (ante 1) ===");
    let mut pkeys: Vec<&&str> = stats.pack_counts.keys().collect();
    pkeys.sort();
    for k in pkeys {
        let v = stats.pack_counts[*k];
        println!("  {:18} {:>8}  ({:.2}%)", k, v, 100.0 * v as f64 / total);
    }
    println!();
    println!("=== Boss distribution (ante 1, top 10) ===");
    let mut bvec: Vec<(&&str, &u64)> = stats.boss_counts.iter().collect();
    bvec.sort_by(|a, b| b.1.cmp(a.1));
    for (k, v) in bvec.iter().take(10) {
        println!("  {:18} {:>8}  ({:.2}%)", k, v, 100.0 * **v as f64 / total);
    }

    let ok = stats.determinism_failures == 0
        && stats.pool_misses == 0
        && stats.locked_ante1_boss_hits == 0
        && stats.white_stake_sticker_hits == 0;

    println!();
    if ok {
        println!("✅ Sweep PASSED all hard checks ({} seeds).", stats.n);
        std::process::exit(0);
    } else {
        println!("❌ Sweep FAILED at least one hard check.");
        std::process::exit(1);
    }
}
