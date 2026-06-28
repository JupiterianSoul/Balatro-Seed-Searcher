# Architecture

The engine is structured as four concentric layers. Each layer is
self-contained, well-tested, and replaceable.

```
┌───────────────────────────────────────────────────────────┐
│  4. UI layer (React)                                      │
│     FilterBuilder → orchestrator → ResultsPanel           │
├───────────────────────────────────────────────────────────┤
│  3. Coordination layer (TypeScript)                       │
│     Web Workers • SharedArrayBuffer • WebGPU adapter      │
│     • Cloud WebSocket adapter (optional, off by default)  │
├───────────────────────────────────────────────────────────┤
│  2. Search loop (Rust)                                    │
│     Filter bytecode • selectivity reordering • scoring    │
├───────────────────────────────────────────────────────────┤
│  1. Determinism core (Rust)                               │
│     pseudohash • LuaRandom • Instance + cache             │
│     Item tables • Source derivations                      │
└───────────────────────────────────────────────────────────┘
```

## Layer 1 — Determinism core

This layer is the only place that touches Balatro's RNG math, and it
ports the reference verbatim. Tests verify that:

- `pseudohash` produces the same f64 bits for the same input every time
- `LuaRandom::from_seed(d)` matches the LuaJIT init sequence
- Two `Instance::new(seed)` objects produce identical sequences

### The pseudohash function

```rust
fn pseudohash(s: &str) -> f64 {
    let mut num = 1.0;
    let shift = (1u64 << 32) as f64;
    for i in (0..s.len()).rev() {
        let ch = s.as_bytes()[i] as f64;
        let a = 1.1239285023 / num * ch * PI;
        let b = PI * (i as f64 + 1.0);
        let scaled = (a + b) * shift;
        let int_part = scaled.floor() as i64;
        let fract_part = fract(fract(a * shift) + fract(b * shift));
        num = fract((int_part as f64 + fract_part) / shift);
    }
    num
}
```

The split-int-and-fract trick is essential: the naive
`num = fract(1.1239285023 / num * ch * PI + PI * (i+1))` diverges from
the game's f64 arithmetic past ~15 sig figs because of how the CPU rounds
the intermediate sum. The reference splits the computation so the
rounding happens in the same place LuaJIT puts it.

### The instance cache

Every "draw" the game performs is keyed by `(type, source, ante, resample)`.
First time a key is seen, the engine:

1. Renders the key to a string: `"raritysho1"`, `"shop_pack" + ante`, etc.
2. Hashes `key_str + seed_str` with pseudohash
3. Stores the result in a 64-entry cache

Each subsequent call to the same key mutates the cached state by
`* 1.72431234 + 2.134453429141 mod 1`, then averages with the run's
hashed seed and seeds a fresh `LuaRandom` from that. The LuaRandom
then yields the actual draw.

This means **the order of operations per seed matters**. Querying
"ante 1 shop joker" before "ante 1 boss" must produce the same results
as querying them in reverse, because the cache is keyed by content,
not call order. This is why the engine builds a fresh `Instance` per
clause evaluation (the worker amortises the cost over many seeds).

## Layer 2 — Search loop

The search loop is the hot path. Per seed:

1. Increment the seed buffer (no allocation)
2. Construct a fresh `Instance` (zeroes a 64-entry cache)
3. Run filter bytecode top-to-bottom, short-circuiting on first failure
4. If all clauses pass, emit a `Match`

The filter compiler:
- Parses the user's JSON filter into a `Filter` struct
- Lowers each clause to an `Op` (compact bytecode)
- Reorders ops by static selectivity (rare > common selectivity)
- Annotates each op with its derivation depth (which sources it triggers)

Future improvement: dynamic selectivity. Workers measure each clause's
rejection rate over the first 10k seeds and re-order based on observed
selectivity. For long searches this can be 2-10× faster than static
ordering.

## Layer 3 — Coordination

Three execution paths, all behind a unified `SearchEngine` interface:

### WASM Worker path (default)

- N workers (`navigator.hardwareConcurrency`, capped at 8)
- Each owns its own WASM instance
- Seed space partitioned by interleaving: worker `i` gets ranks `i, i+N, i+2N, ...`
- Workers post `{matches, scanned}` batches every ~250ms
- Main thread aggregates and de-duplicates by rank

### WebGPU path (opt-in, experimental)

GPU is great at pseudohash (lots of f32 mul-add) but is **not bit-exact**
with the CPU f64 path. We use it as a **pre-filter**:

1. GPU dispatches 65k seeds against a SINGLE clause (typically the most
   selective one, e.g. "ante-1 boss = X")
2. GPU emits a pass-bit per seed
3. Main thread reads survivors (~0.1% of input for selective clauses)
4. CPU re-validates each survivor against the FULL filter

This pattern preserves correctness while exploiting GPU throughput.
On a laptop iGPU (e.g. Intel UHD 770), one pre-filter dispatch handles
~10× more seeds in the same wall time as one WASM worker.

### Cloud path (opt-in, off by default)

A self-hosted Node WebSocket server can run native Rust workers (no
WASM overhead). The user's browser connects, sends scan chunks, receives
matches. Disabled by default per the project's "no required infra"
constraint. See `server/README.md`.

## Layer 4 — UI

React components. The orchestrator exposes an EventTarget-style API
that components subscribe to:

```ts
orchestrator.addEventListener('match', (e: CustomEvent<Match>) => { ... });
orchestrator.addEventListener('progress', (e: CustomEvent<Progress>) => { ... });
orchestrator.start(filter, runConfig);
```

Components are stateless except for local UI state — the orchestrator
owns the source of truth.

## Why this architecture?

- **The determinism core is the smallest possible surface.** Any bug
  here corrupts every result, so it gets the most tests and the
  cleanest dependency boundary.
- **The search loop is platform-agnostic.** Same code runs native, in
  WASM, in a Node worker, and in a Deno edge function (when the cloud
  path lands).
- **Workers don't share state.** Each worker owns its WASM instance
  and is responsible for its own seed range. The only cross-worker
  communication is via `postMessage` (or `SharedArrayBuffer` when
  available, but the protocol is identical).
- **The UI is replaceable.** The engine + orchestrator could be lifted
  out and dropped into Balatropedia, a Discord bot, or a CLI tool
  without modification.

## Comparison vs Immolate

| Concern | Immolate (CPU) | Immolate (OpenCL) | This engine |
|---|---|---|---|
| Language | C | OpenCL C | Rust |
| Browser target | none (native CLI) | none (needs GPU drivers) | WASM + SIMD + WebGPU |
| Filter format | hand-coded C files, recompiled per filter | same | JSON DSL, compiled at runtime |
| Parallelism | one process per core | one workgroup per ~10k seeds | N workers + N GPU dispatches |
| Streaming results | no | no | yes (Web Worker postMessage) |
| Closeness scoring | no | no | yes (partial match mode) |
| Verified reproduction | no | no | planned (milestone 3) |
| Lines of code | ~4,000 (functions.cl) | same | ~1,500 (Rust, growing) |
