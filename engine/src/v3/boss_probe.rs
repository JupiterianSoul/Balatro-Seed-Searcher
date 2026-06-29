//! V3 reference: the boss probe, computed via DF arithmetic.
//!
//! This is the simplest "real" filter op in the engine. It takes a seed
//! string and an ante, and asks: "what boss is at this ante?"
//!
//! The boss is selected by:
//!   1. Build node key "bossA{ante}" (or similar — actual key per game).
//!   2. Apply ante locks (some bosses are gated behind antes).
//!   3. randchoice with the unlocked boss list via LuaRandom seeded from
//!      pseudohash(key + seed).
//!
//! This module implements steps 1+3 via DF arithmetic. Ante-locked boss
//! exclusion is a finite set lookup that's trivial to translate.
//!
//! NOTE: This intentionally uses a *simplified* boss-selection rule that
//! ignores the node cache state-mutation (the `* 1.72431234 + ...` step
//! the full Instance does). The full state machine is out of scope for
//! the V3 GPU port; the parity harness measures how often this simplified
//! rule agrees with the production V2 engine on the boss-at-ante-1 probe.

use super::lua_random::LuaRandomDf;
use super::pseudohash::pseudohash_df;
use crate::tables::BOSSES;

/// All 28 bosses in the game, sorted alphabetically for stable ordering.
/// This mirrors `crate::tables::BOSSES`.
pub fn boss_at_ante_df(seed: &str, ante: u32) -> &'static str {
    // Build the key for the boss draw: "boss<ante>" + seed string.
    let key = format!("boss{}{}", ante, seed);
    let hashed = pseudohash_df(key.as_bytes());
    let mut rng = LuaRandomDf::from_seed_df(hashed);

    // Filter out bosses locked at this ante.
    let mut pool: Vec<&'static str> = BOSSES
        .iter()
        .copied()
        .filter(|b| !is_boss_locked(b, ante))
        .collect();
    pool.sort_unstable();

    if pool.is_empty() {
        return "";
    }
    let idx = rng.next_int(pool.len() as u32) - 1;
    pool[idx as usize]
}

fn is_boss_locked(name: &str, ante: u32) -> bool {
    if ante < 2 && matches!(name,
        "The Mouth" | "The Fish" | "The Wall" | "The House" |
        "The Mark" | "The Wheel" | "The Arm" | "The Water" |
        "The Needle" | "The Flint") { return true; }
    if ante < 3 && matches!(name, "The Tooth" | "The Eye") { return true; }
    if ante < 4 && name == "The Plant" { return true; }
    if ante < 5 && name == "The Serpent" { return true; }
    if ante < 6 && name == "The Ox" { return true; }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boss_at_ante_df_is_deterministic() {
        for ante in 1..=8 {
            let a = boss_at_ante_df("ABCD1234", ante);
            let b = boss_at_ante_df("ABCD1234", ante);
            assert_eq!(a, b);
        }
    }

    #[test]
    fn boss_at_ante_df_returns_valid_boss() {
        for ante in 1..=8 {
            let b = boss_at_ante_df("ABCD1234", ante);
            assert!(BOSSES.contains(&b), "got invalid boss: {}", b);
        }
    }

    /// Document the divergence rate between DF and f64 boss selection.
    /// We expect SOME divergence because:
    ///   1. DF arithmetic isn't bit-identical to f64.
    ///   2. The boss probe here uses a simplified rule; the real V2
    ///      engine applies node cache state mutation.
    /// The parity harness in `scripts/` measures and logs this.
    #[test]
    fn df_boss_divergence_is_documented() {
        let n = 1000;
        let mut matches = 0;
        let mut total = 0;
        for i in 0..n {
            let seed = format!("SEED{:04}", i);
            for ante in 1..=4u32 {
                let df_boss = boss_at_ante_df(&seed, ante);

                // Build f64-side ground truth using the SAME simplified rule
                // (so we're measuring DF arithmetic vs native f64, not
                // DF vs the production V2 cache machine).
                let key = format!("boss{}{}", ante, seed);
                let hashed = crate::rng::pseudohash(&key);
                let mut rng = crate::rng::LuaRandom::from_seed(hashed);

                let mut pool: Vec<&'static str> = BOSSES.iter().copied()
                    .filter(|b| !is_boss_locked(b, ante))
                    .collect();
                pool.sort_unstable();
                let idx = rng.next_int(pool.len() as u32) - 1;
                let f64_boss = pool[idx as usize];

                total += 1;
                if df_boss == f64_boss { matches += 1; }
            }
        }
        let agree_rate = matches as f64 / total as f64;
        eprintln!(
            "DF boss probe agreement with f64 (simplified rule): {}/{} = {:.4}",
            matches, total, agree_rate
        );
        // We don't assert a specific rate — we just log it. The harness in
        // scripts/parity_v3.js will measure this against the real V2 engine.
    }
}
