# V3 design notes — WebGPU sprint

Status: shipped as beta on 2026-06-29.
Author: engineering sprint with Julie.

This document records what V3 actually ships, the precision finding that
forced the scope to change mid-sprint, and what would unblock real
GPU-side seed search in a future version.

## TL;DR

* **V3 ships:** WebGPU device detection, an end-to-end-verified
  diagnostic compute shader (integer tausworthe), an engine selector
  with a clean WebGPU → WASM-SIMD → WASM-scalar fallback chain, and a
  beta toggle in both web apps. All search workloads still run on the
  WASM CPU engine.
* **V3 does NOT yet run seed searches on the GPU.** The original plan
  ("f32 + CPU verify, the best of both worlds") turned out to be
  unworkable for this particular RNG, for the reason described below.
* **Next sprint, if approved:** full f64 emulation on the GPU (double-
  float arithmetic across the whole `pseudohash → LuaRandom` chain) or
  a hybrid scheme that keeps the precision-critical step on the CPU.

## What was supposed to happen

The plan we agreed on before the work started:

1. Port the hot path (`pseudohash`, `LuaRandom`, ante/boss probe) to
   WGSL using `f32` for speed.
2. The GPU produces a stream of candidate seeds that pass a coarse
   filter.
3. The CPU re-runs the survivors with the canonical `f64` engine, so
   the user only ever sees verified hits.

It looked safe because `f32` has 24 mantissa bits, and any divergence
from `f64` would just produce noise that the CPU verifier filters out.

## What actually happened

Implemented in `engine/src/v3/{df,pseudohash,lua_random,boss_probe}.rs`,
gated behind `cargo test --release`. The DF (double-float) arithmetic
uses Dekker / Knuth two-sum and two-product, the standard "two f32s
emulating ~48 mantissa bits" trick. DF is meaningfully more precise
than raw `f32` and should have been a comfortable margin.

It still wasn't enough.

### Test 1: DF vs f64 pseudohash outputs

| Input seed | f64 result | DF result | absolute gap |
| --- | --- | --- | --- |
| `"AAAAAAAA"` (8-char) | 0.7314... | 0.4127... | 0.319 |
| Sample of 1000 chained seeds | mean gap 0.41 | | |

The pseudohash mixes mul/div in `[0, 1)` repeatedly. Tiny rounding
errors accumulate non-linearly and the DF stream drifts to
uncorrelated values within ~50 iterations. Source: `pseudohash.rs`
tests.

### Test 2: Boss probe agreement rate vs f64

`engine/src/v3/boss_probe.rs` runs both implementations on the same
1000 seeds and compares which boss is selected for ante 8.

* Agreement rate: **4.3 %** (essentially the chance of random
  collision between 24-ish bosses).

This number is the smoking gun. If DF were "approximately right" the
agreement rate would be much higher than chance, and a CPU verifier
could fish the real hits out of an enlarged candidate pool. At 4.3 %
the GPU isn't producing a noisy version of the answer, it's producing
unrelated answers. There is no "verify the survivors" strategy that
recovers correctness when the GPU stream and the CPU stream agree at
chance.

### Why does this happen — the to_bits() amplifier

`LuaRandom::from_seed` (modeled on the Lua 5.1 `random` reseed path)
does:

```text
let n = f64::from_bits((pseudohash(seed) * MIX_CONSTANT) as u64);
```

Once a `f64`'s bits are reinterpreted as a `u64`, **any** mantissa
bit difference becomes a totally different state. The downstream RNG
stream is therefore not "close" to the reference stream — it's
unrelated. f32 mantissa noise gets amplified into a different RNG
entirely.

You cannot fix this by being a bit more precise. You either get the
mantissa bit-exact, or you get noise.

## What V3 actually ships

Given the finding above, three options were possible:

1. Ship the broken GPU path and lie about it.
2. Ship nothing and call the sprint a wash.
3. Ship the honest scaffold — verified WebGPU plumbing plus a real
   benchmark — and document the blocker so the next sprint has a
   clean starting point.

V3 = option 3.

### Components

* `engine/src/v3/df.rs` — double-float arithmetic primitives, used by
  the parity tests. ~270 lines, 33 / 33 tests passing.
* `engine/src/v3/diagnostic.rs` — integer-only tausworthe RNG that
  runs on the CPU as a reference.
* `engine/shaders/diagnostic.wgsl` — the same tausworthe, written in
  WGSL. Validated by `naga`. Runs in a 64-thread workgroup; one
  output u32 per thread, verified byte-for-byte against the CPU
  reference inside the browser.
* `engine/src/wasm_api.rs` — exports `v3_diagnostic_cpu(seed_base,
  iter_count, seed_count) -> Uint32Array` and
  `v3_diagnostic_shader_source() -> string`. The shader is shipped
  with the WASM bundle to avoid an extra fetch.
* `client/src/lib/v3/webgpuEngine.ts` (Balatropedia) and
  `web/src/v3/webgpuEngine.ts` (standalone) — call `requestAdapter`,
  compile the shader, dispatch, read back, verify against
  `v3_diagnostic_cpu`. Returns a tagged status:
  * `unsupported` — no `navigator.gpu` or no adapter
  * `verification-failed` — shader compiled but its output diverges
    from CPU reference (driver bug)
  * `ready` — everything matches; reports adapter info and a rough
    "ops/sec" throughput
* `client/src/lib/v3/engineSelector.ts` (Balatropedia) and
  `web/src/v3/engineSelector.ts` (standalone) — selects an engine
  descriptor with fallback. **`searchBackend` is always `'wasm'` in
  V3.** This is the explicit, type-level acknowledgement that V3
  doesn't run searches on the GPU.
* `SeedFinderTab.tsx` (Balatropedia) and `App.tsx` (standalone) —
  hide the V3 toggle by default. Reveal it via `?v3=1` in the URL or
  via a previously-saved `localStorage["…-v3-beta"] === "on"`. State
  syncs to localStorage so a user who opted in stays in.

### Why a diagnostic shader rather than a stub

A real CPU-↔-GPU bit-parity check proves three things in one shot:

1. WebGPU is genuinely available in the user's browser/WebView.
2. The shader the WASM bundle ships compiles on the user's driver.
3. The GPU stack produces numerically correct output on this device.

If we ever do ship real GPU search, items 1–3 are the exact same
checks we'll need to gate it on. The diagnostic shader is therefore
not throwaway code; it's the device-acceptance test for the real
shader.

A stub that always returns "ok" would lie about progress, and a
stub that always returns "skip" would teach us nothing about the
hardware out there.

## Performance numbers from V3 build

(measured on the CI sandbox during the sprint, your hardware will
differ)

| Engine                | Hot-path throughput | Notes |
| --- | --- | --- |
| V1 Immolate WASM      | ~25 k seeds/sec      | Single-threaded, original engine |
| V2 WASM-SIMD          | ~210 k seeds/sec     | Current default in both web apps |
| V3 diagnostic shader  | ~10–50 M tausworthe ops/sec | GPU integer throughput, not seeds |

The V3 row is **not directly comparable** to V1/V2 because the
diagnostic shader runs integer tausworthe, not the real pseudohash
chain. It does tell us what raw lane throughput the GPU can sustain
once a real shader gets there — somewhere between 50x and 200x the
V2 SIMD rate on typical discrete GPUs, ballpark.

## Paths forward (in cost order)

1. **Full f64 emulation in WGSL.** Quad-float or true f64 software in
   the shader so that `pseudohash` and `LuaRandom::from_seed`
   reproduce the reference `f64` results bit-for-bit. Multi-week
   effort. Result: real GPU search, ~50–100x speedup over V2.

2. **Hybrid CPU+GPU.** GPU computes the cheap part of the filter
   (card draws, joker rolls that don't depend on `to_bits`), CPU
   computes the precision-critical step. The savings depend heavily
   on which filters the user enabled — useless for boss-only
   queries, large for joker-heavy queries.

3. **Native `f64` WGSL extension.** A handful of adapters expose the
   `"f64"` shader feature (`requestDevice({ requiredFeatures: ['f64']
   })`). When available, no emulation is needed. Coverage is too
   spotty to ship as a default but is a worthwhile fast path on top
   of (1).

4. **Replace `to_bits()` in our RNG.** Not viable — the to_bits
   step is part of how Balatro itself seeds, and changing it would
   make our results diverge from the game.

## Migration / rollback

* V3 is gated behind `?v3=1` and the localStorage flag. Default users
  see no change.
* Even with V3 enabled, `searchBackend === 'wasm'` is enforced at the
  type level, so worst case a V3 user gets a slightly slower page
  load (one extra wasm import on probe) plus the diagnostic shader
  status line.
* To remove V3 entirely we delete the `v3/` directories and the
  toggle blocks; the rest of the codebase is untouched.

## Test coverage

* `cargo test --release` — 33 / 33 passing, including the DF parity
  tests that document (rather than assert) the f64-vs-DF divergence.
* `naga engine/shaders/diagnostic.wgsl` — validation successful.
* `scripts/parity_bitwise.js 10000` — V2 WASM still bit-for-bit
  identical to the reference Rust output across 10 000 seeds.
* Browser-side runtime check: `probeWebGpu` runs the shader and
  refuses to report `ready` unless every one of 4096 output lanes
  matches the WASM reference exactly.
