// V3 diagnostic / capability shader.
//
// Runs Balatro's LuaJIT tausworthe step in parallel across many GPU threads
// purely as an integer-only workload. This does NOT match the f64
// pseudohash-seeded RNG used by the production engine — see docs/V3_DESIGN.md
// for why. It exists to:
//   1. Validate end-to-end WebGPU plumbing in browser + APK WebView.
//   2. Measure raw integer throughput of the user's GPU vs WASM CPU.
//   3. Provide a CPU-verifiable workload (the same tausworthe in Rust
//      reaches the same output bits) for confidence in the GPU stack.
//
// Each invocation owns one tausworthe lane state (4 u64 lanes emulated as
// 8 u32 halves) and runs `iter_count` steps, then writes the final mixed
// output to a result buffer. The host reads back, sums, and reports a
// throughput number.
//
// Validated with naga 26.x WGSL frontend.

struct Params {
    seed_base : u32,
    iter_count : u32,
    seed_count : u32,
    _pad : u32,
};

@group(0) @binding(0) var<uniform> params : Params;
@group(0) @binding(1) var<storage, read_write> results : array<u32>;

// ---------------------------------------------------------------------------
// u64 emulation. We represent a u64 as vec2<u32>(lo, hi).
// ---------------------------------------------------------------------------

fn u64_xor(a: vec2<u32>, b: vec2<u32>) -> vec2<u32> {
    return vec2<u32>(a.x ^ b.x, a.y ^ b.y);
}

fn u64_shl(a: vec2<u32>, n: u32) -> vec2<u32> {
    // 0 < n < 64 assumed. WGSL shift is well-defined for shift < 32 only.
    if (n == 0u) { return a; }
    if (n >= 32u) {
        let s = n - 32u;
        // safety: if s == 0 (n==32) lo<<0 == lo
        return vec2<u32>(0u, a.x << s);
    } else {
        let hi_low = a.y << n;
        let hi_carry = a.x >> (32u - n);
        let lo = a.x << n;
        return vec2<u32>(lo, hi_low | hi_carry);
    }
}

fn u64_shr(a: vec2<u32>, n: u32) -> vec2<u32> {
    if (n == 0u) { return a; }
    if (n >= 32u) {
        let s = n - 32u;
        return vec2<u32>(a.y >> s, 0u);
    } else {
        let lo_high = a.x >> n;
        let lo_borrow = a.y << (32u - n);
        let hi = a.y >> n;
        return vec2<u32>(lo_high | lo_borrow, hi);
    }
}

fn u64_and(a: vec2<u32>, b: vec2<u32>) -> vec2<u32> {
    return vec2<u32>(a.x & b.x, a.y & b.y);
}

// Constant !0u64 << k — used as a "clear low k bits" mask in tausworthe.
// Returns vec2<u32>(lo_mask, hi_mask).
fn mask_clear_low(k: u32) -> vec2<u32> {
    if (k == 0u) { return vec2<u32>(0xFFFFFFFFu, 0xFFFFFFFFu); }
    if (k >= 64u) { return vec2<u32>(0u, 0u); }
    if (k >= 32u) {
        let s = k - 32u;
        // lo = 0, hi has lower s bits cleared
        let hi = 0xFFFFFFFFu << s;
        return vec2<u32>(0u, hi);
    } else {
        let lo = 0xFFFFFFFFu << k;
        return vec2<u32>(lo, 0xFFFFFFFFu);
    }
}

// ---------------------------------------------------------------------------
// LuaJIT tausworthe step. State is four u64 lanes; output is the XOR of
// all four lanes after one step.
//
// Constants match `engine/src/rng.rs` exactly.
// ---------------------------------------------------------------------------

fn taus_step(state: ptr<function, array<vec2<u32>, 4>>) -> vec2<u32> {
    var r = vec2<u32>(0u, 0u);

    // Lane 0: z = (((z << 31) ^ z) >> 45) ^ ((z & (~0u64 << 1)) << 18)
    var z = (*state)[0];
    let lo0 = u64_shr(u64_xor(u64_shl(z, 31u), z), 45u);
    let hi0 = u64_shl(u64_and(z, mask_clear_low(1u)), 18u);
    z = u64_xor(lo0, hi0);
    r = u64_xor(r, z);
    (*state)[0] = z;

    // Lane 1
    z = (*state)[1];
    let lo1 = u64_shr(u64_xor(u64_shl(z, 19u), z), 30u);
    let hi1 = u64_shl(u64_and(z, mask_clear_low(6u)), 28u);
    z = u64_xor(lo1, hi1);
    r = u64_xor(r, z);
    (*state)[1] = z;

    // Lane 2
    z = (*state)[2];
    let lo2 = u64_shr(u64_xor(u64_shl(z, 24u), z), 48u);
    let hi2 = u64_shl(u64_and(z, mask_clear_low(9u)), 7u);
    z = u64_xor(lo2, hi2);
    r = u64_xor(r, z);
    (*state)[2] = z;

    // Lane 3
    z = (*state)[3];
    let lo3 = u64_shr(u64_xor(u64_shl(z, 21u), z), 39u);
    let hi3 = u64_shl(u64_and(z, mask_clear_low(17u)), 8u);
    z = u64_xor(lo3, hi3);
    r = u64_xor(r, z);
    (*state)[3] = z;

    return r;
}

// ---------------------------------------------------------------------------
// Entry point: per-invocation, init state from (seed_base + global_id) and
// run iter_count steps. Write final XOR mix to results[global_id].
// ---------------------------------------------------------------------------

@compute @workgroup_size(64)
fn benchmark(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if (i >= params.seed_count) { return; }

    // Seed the four lanes from a simple bit-mix of (seed_base + i).
    // We use a stable, well-distributed seeding (xorshift64*) so different
    // GPU lanes start from different non-trivial states.
    let seed = params.seed_base + i;
    var state : array<vec2<u32>, 4>;

    var s0 = vec2<u32>(seed ^ 0x9E3779B9u, seed * 0x85EBCA77u + 0x123456u);
    state[0] = s0;
    var s1 = vec2<u32>(seed * 0xC2B2AE3Du, seed ^ 0x27D4EB2Fu);
    state[1] = s1;
    var s2 = vec2<u32>(seed ^ 0x165667B1u, seed * 0x9E3779B9u + 0xABCDu);
    state[2] = s2;
    var s3 = vec2<u32>(seed * 0x6C8E944Fu, seed ^ 0xCC9E2D51u);
    state[3] = s3;

    var acc = vec2<u32>(0u, 0u);
    for (var step: u32 = 0u; step < params.iter_count; step = step + 1u) {
        acc = u64_xor(acc, taus_step(&state));
    }

    // Mix and store. We write a single u32 (lo XOR hi) to keep buffer size small.
    results[i] = acc.x ^ acc.y;
}
