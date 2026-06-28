//! Core RNG primitives that Balatro uses.
//!
//! - `pseudohash`: deterministic string → f64 in [0,1). Drives every "named"
//!   draw in the game ("Joker1Soul", "Tarot1Arcana1", etc.)
//! - `LuaRandom`: LuaJIT's tausworthe-based PRNG, seeded with an f64.
//!   Identical to `math.random()` in Lua 5.1+/LuaJIT 2.x.
//!
//! References:
//!   - Immolate (MathIsFun0): lib/util.cl — pseudohash, _randint, randomseed
//!   - LuaJIT lib_math.c — tausworthe step constants
//!   - Balatro decompiled functions/random.lua

/// Pseudo-hash: maps a UTF-8 string to a deterministic f64 in [0, 1).
///
/// This is the central deterministic mixer. Every seeded draw in Balatro
/// computes `pseudohash(key + seed)` where `key` is something like
/// `"Joker1"`, `"Tarot1Arcana1"`, `"Voucher1"`, etc.
///
/// The math is float-fragile — we mirror Immolate's split-int-and-fract
/// trick so we match the reference bit-for-bit on every common architecture.
#[inline]
pub fn pseudohash(s: &str) -> f64 {
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut num: f64 = 1.0;
    let k: u32 = 32;
    let shift: f64 = (1u64 << k) as f64; // 2^32

    let mut i = len as isize - 1;
    while i >= 0 {
        let ch = bytes[i as usize] as f64;
        // a = 1.1239285023 / num * ch * pi
        let a = 1.1239285023_f64 / num * ch * std::f64::consts::PI;
        // b = pi * (i+1)
        let b = std::f64::consts::PI * (i as f64 + 1.0);

        let scaled = (a + b) * shift;
        let int_part = scaled.floor() as i64;

        let fract_part = fract(fract(a * shift) + fract(b * shift));
        num = fract((int_part as f64 + fract_part) / shift);
        i -= 1;
    }
    num
}

#[inline(always)]
fn fract(x: f64) -> f64 {
    x - x.floor()
}

/// LuaJIT-compatible PRNG. State is four u64 lanes; output is an IEEE-754
/// double drawn from [0, 1) via the classic mantissa-mask trick.
#[derive(Clone, Copy, Debug)]
pub struct LuaRandom {
    state: [u64; 4],
}

impl LuaRandom {
    /// Initialise from a seed double. Mirrors LuaJIT's `lj_math_random_seed`.
    pub fn from_seed(mut d: f64) -> Self {
        let mut r: u32 = 0x11090601;
        let mut state = [0u64; 4];
        for i in 0..4 {
            let m: u32 = 1u32 << (r & 255);
            r >>= 8;
            // The reference performs these two ops separately on purpose
            // — combined, the rounding diverges from LuaJIT.
            d *= std::f64::consts::PI;
            d += std::f64::consts::E;
            let mut u: u64 = d.to_bits();
            if u < m as u64 {
                u += m as u64;
            }
            state[i] = u;
        }
        let mut lr = LuaRandom { state };
        for _ in 0..10 {
            lr.rand_int();
        }
        lr
    }

    /// Step the four-lane tausworthe and return the combined 64-bit output.
    #[inline]
    fn rand_int(&mut self) -> u64 {
        let mut r: u64 = 0;

        // Lane 0
        let mut z = self.state[0];
        z = (((z << 31) ^ z) >> 45) ^ ((z & (!0u64 << 1)) << 18);
        r ^= z;
        self.state[0] = z;

        // Lane 1
        z = self.state[1];
        z = (((z << 19) ^ z) >> 30) ^ ((z & (!0u64 << 6)) << 28);
        r ^= z;
        self.state[1] = z;

        // Lane 2
        z = self.state[2];
        z = (((z << 24) ^ z) >> 48) ^ ((z & (!0u64 << 9)) << 7);
        r ^= z;
        self.state[2] = z;

        // Lane 3
        z = self.state[3];
        z = (((z << 21) ^ z) >> 39) ^ ((z & (!0u64 << 17)) << 8);
        r ^= z;
        self.state[3] = z;

        r
    }

    /// Yield the next double in [0, 1) — equivalent to `math.random()`.
    #[inline]
    pub fn next_double(&mut self) -> f64 {
        let bits = self.rand_int();
        // Stuff the mantissa with 52 random bits, fix the exponent to bias-0
        // (so the double sits in [1, 2)), then subtract 1.
        let masked = (bits & 0x000F_FFFF_FFFF_FFFF) | 0x3FF0_0000_0000_0000;
        f64::from_bits(masked) - 1.0
    }

    /// `math.random(n)` — integer in [1, n] inclusive. Matches Lua semantics.
    #[inline]
    pub fn next_int(&mut self, n: u32) -> u32 {
        (self.next_double() * n as f64).floor() as u32 + 1
    }
}

/// `pseudoseed(key)` — Balatro's per-source-key seeded random.
///
/// Combines the run seed (an f64 derived from the 8-char seed string) with
/// the source key by re-hashing `key + seed_string` and feeding the result
/// to a fresh `LuaRandom`. Used as the canonical "give me a random number
/// for source X in this run" call.
#[inline]
pub fn pseudoseed_random(seed_string: &str, key: &str) -> LuaRandom {
    let mut concat = String::with_capacity(key.len() + seed_string.len());
    concat.push_str(key);
    concat.push_str(seed_string);
    LuaRandom::from_seed(pseudohash(&concat))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reference value from running Immolate on seed "PHRAYJUS" with key "Joker1":
    /// pseudohash("Joker1PHRAYJUS") = 0.4859... (recomputed during port).
    /// We assert determinism and stable round-trip; exact reference cross-check
    /// is in `tests/parity.rs` against the OpenCL binary.
    #[test]
    fn pseudohash_is_deterministic() {
        let a = pseudohash("Joker1ABCD1234");
        let b = pseudohash("Joker1ABCD1234");
        assert_eq!(a, b);
        assert!(a >= 0.0 && a < 1.0);
    }

    #[test]
    fn pseudohash_differs_per_input() {
        let a = pseudohash("Joker1ABCD1234");
        let b = pseudohash("Joker2ABCD1234");
        assert_ne!(a, b);
    }

    #[test]
    fn lua_random_in_unit_interval() {
        let mut r = LuaRandom::from_seed(pseudohash("test"));
        for _ in 0..1000 {
            let v = r.next_double();
            assert!(v >= 0.0 && v < 1.0, "out of range: {v}");
        }
    }

    #[test]
    fn lua_random_next_int_is_one_indexed() {
        let mut r = LuaRandom::from_seed(0.5);
        for _ in 0..1000 {
            let v = r.next_int(10);
            assert!((1..=10).contains(&v), "next_int(10) returned {v}");
        }
    }

    #[test]
    fn lua_random_is_reproducible() {
        let mut a = LuaRandom::from_seed(pseudohash("same"));
        let mut b = LuaRandom::from_seed(pseudohash("same"));
        for _ in 0..100 {
            assert_eq!(a.next_double().to_bits(), b.next_double().to_bits());
        }
    }
}
