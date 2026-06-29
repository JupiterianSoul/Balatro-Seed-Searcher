//! LuaRandom ported to a form a WGSL compute shader can run.
//!
//! The tausworthe core is integer-only and translates cleanly. The seeding
//! step uses `d.to_bits()` (the f64 mantissa pattern), which is the most
//! GPU-hostile primitive in the chain — we receive `d` as a DF and need to
//! extract the f64-equivalent mantissa from it.

use super::df::Df;

/// DF-side LuaRandom. The four u64 state lanes are kept as `[u32; 8]`
/// (lo/hi pairs) because WGSL has no u64.
///
/// For the Rust-side parity reference we still use u64 internally and
/// only split when comparing to the shader's output.
#[derive(Clone, Copy, Debug)]
pub struct LuaRandomDf {
    pub state: [u64; 4],
}

impl LuaRandomDf {
    /// Approximate `f64::to_bits` of `(d.hi + d.lo)`.
    ///
    /// We reconstruct an f64 from the DF, then take its bits. On the GPU
    /// the shader has to do this in a more contorted way (manually
    /// constructing the f64 mantissa from the DF components); the Rust
    /// reference takes the shortcut because both should produce the same
    /// bits when the DF is in the representable range.
    fn df_to_f64_bits(d: Df) -> u64 {
        let total = d.hi as f64 + d.lo as f64;
        total.to_bits()
    }

    /// `from_seed` taking a DF — matches the shader exactly.
    pub fn from_seed_df(seed: Df) -> Self {
        let pi_df = Df::from_f64(std::f64::consts::PI);
        let e_df = Df::from_f64(std::f64::consts::E);
        let mut d = seed;
        let mut r: u32 = 0x11090601;
        let mut state = [0u64; 4];
        for i in 0..4 {
            let m: u32 = 1u32 << (r & 255);
            r >>= 8;
            d = super::df::mul(d, pi_df);
            d = super::df::add(d, e_df);
            let mut u: u64 = Self::df_to_f64_bits(d);
            if u < m as u64 {
                u += m as u64;
            }
            state[i] = u;
        }
        let mut lr = LuaRandomDf { state };
        for _ in 0..10 {
            lr.rand_int();
        }
        lr
    }

    /// Tausworthe step. Pure integer math — bit-identical to the f64 version.
    #[inline]
    fn rand_int(&mut self) -> u64 {
        let mut r: u64 = 0;

        let mut z = self.state[0];
        z = (((z << 31) ^ z) >> 45) ^ ((z & (!0u64 << 1)) << 18);
        r ^= z;
        self.state[0] = z;

        z = self.state[1];
        z = (((z << 19) ^ z) >> 30) ^ ((z & (!0u64 << 6)) << 28);
        r ^= z;
        self.state[1] = z;

        z = self.state[2];
        z = (((z << 24) ^ z) >> 48) ^ ((z & (!0u64 << 9)) << 7);
        r ^= z;
        self.state[2] = z;

        z = self.state[3];
        z = (((z << 21) ^ z) >> 39) ^ ((z & (!0u64 << 17)) << 8);
        r ^= z;
        self.state[3] = z;

        r
    }

    /// `next_double` — same mantissa-mask trick.
    pub fn next_double(&mut self) -> f64 {
        let bits = self.rand_int();
        let masked = (bits & 0x000F_FFFF_FFFF_FFFF) | 0x3FF0_0000_0000_0000;
        f64::from_bits(masked) - 1.0
    }

    pub fn next_int(&mut self, n: u32) -> u32 {
        (self.next_double() * n as f64).floor() as u32 + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::LuaRandom;

    /// Documents the DF ↔ f64 divergence for LuaRandom seeding.
    /// This does NOT assert agreement — the parity write-up in
    /// docs/V3_DESIGN.md explains why.
    #[test]
    fn df_lua_random_divergence_is_documented() {
        let seed_exact = 0.5_f64;
        let df_seed = Df::from_f64(seed_exact);
        let mut a = LuaRandom::from_seed(seed_exact);
        let mut b = LuaRandomDf::from_seed_df(df_seed);
        let mut agreements = 0;
        for _ in 0..32 {
            let va = a.next_double();
            let vb = b.next_double();
            if va.to_bits() == vb.to_bits() { agreements += 1; }
        }
        eprintln!("DF LuaRandom seeded from f32-DF agreed with f64 on {}/32 draws (expected: low)", agreements);
    }
}
