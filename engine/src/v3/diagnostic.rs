//! V3 diagnostic CPU reference.
//!
//! Mirrors `shaders/diagnostic.wgsl` exactly. Used by:
//!   - `scripts/v3_gpu_smoke.js` — runs both CPU and GPU implementations
//!     and compares outputs (any divergence = WebGPU implementation bug or
//!     driver issue).
//!   - WASM workers — when WebGPU is unavailable but the V3 toggle is on,
//!     the diagnostic still runs on CPU so the UI's GPU throughput chip
//!     shows a meaningful "WASM equivalent" number for comparison.

#[inline]
fn taus_step(state: &mut [u64; 4]) -> u64 {
    let mut r: u64 = 0;

    let mut z = state[0];
    z = (((z << 31) ^ z) >> 45) ^ ((z & (!0u64 << 1)) << 18);
    r ^= z;
    state[0] = z;

    z = state[1];
    z = (((z << 19) ^ z) >> 30) ^ ((z & (!0u64 << 6)) << 28);
    r ^= z;
    state[1] = z;

    z = state[2];
    z = (((z << 24) ^ z) >> 48) ^ ((z & (!0u64 << 9)) << 7);
    r ^= z;
    state[2] = z;

    z = state[3];
    z = (((z << 21) ^ z) >> 39) ^ ((z & (!0u64 << 17)) << 8);
    r ^= z;
    state[3] = z;

    r
}

#[inline]
fn seed_state(seed: u32) -> [u64; 4] {
    // Match the WGSL seeding pattern bit-for-bit.
    let s = seed;
    let pack = |lo: u32, hi: u32| -> u64 { (lo as u64) | ((hi as u64) << 32) };
    [
        pack(s ^ 0x9E3779B9u32, s.wrapping_mul(0x85EBCA77u32).wrapping_add(0x123456u32)),
        pack(s.wrapping_mul(0xC2B2AE3Du32), s ^ 0x27D4EB2Fu32),
        pack(s ^ 0x165667B1u32, s.wrapping_mul(0x9E3779B9u32).wrapping_add(0xABCDu32)),
        pack(s.wrapping_mul(0x6C8E944Fu32), s ^ 0xCC9E2D51u32),
    ]
}

/// Run the diagnostic workload on CPU; results vector length == seed_count.
/// Each output is a single u32 = lo(acc) XOR hi(acc).
pub fn run_cpu(seed_base: u32, iter_count: u32, seed_count: u32) -> Vec<u32> {
    let mut out = Vec::with_capacity(seed_count as usize);
    for i in 0..seed_count {
        let mut state = seed_state(seed_base + i);
        let mut acc: u64 = 0;
        for _ in 0..iter_count {
            acc ^= taus_step(&mut state);
        }
        out.push((acc as u32) ^ ((acc >> 32) as u32));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_is_deterministic() {
        let a = run_cpu(42, 1000, 256);
        let b = run_cpu(42, 1000, 256);
        assert_eq!(a, b);
    }

    #[test]
    fn diagnostic_diverges_per_seed() {
        let r = run_cpu(0, 100, 256);
        let unique: std::collections::HashSet<u32> = r.iter().copied().collect();
        // 256 lanes should produce mostly-unique outputs
        assert!(unique.len() >= 200, "expected mostly unique outputs, got {}/256", unique.len());
    }
}
