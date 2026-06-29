# Changelog

All notable changes to the Balatro Seed Searcher engine.

## v4.0 — 2026-06-29 (phone-perf sprint — rayon threading + cold-start polish)

GPU path put on hold (see v3.0-beta1; double-float emulation diverges too
much to be useful for boss probe). This release pivots to the actual
goal — a fast Play Store APK — by parallelising the WASM CPU engine
inside a single heap with rayon, and by shaving cold-start latency on
phones.

### Engine
- **`engine/Cargo.toml`** — new `wasm-threads` feature gates
  `wasm-bindgen-rayon` + `rayon`. Built with nightly
  `-Z build-std=std,panic_abort` and
  `RUSTFLAGS='-C target-feature=+atomics,+bulk-memory,+mutable-globals,+simd128'`.
- **`engine/src/search.rs`** — new `scan_parallel` walks the seed-rank
  range with `rayon::par_chunks`, evaluates each seed independently,
  and reduces hits into a single `Vec<SearchHit>`. Each seed gets its
  own `Run` so determinism is preserved.
- **`engine/src/wasm_api.rs`** — re-exports `init_thread_pool` from
  `wasm-bindgen-rayon` and adds `scan_chunk_parallel` with the same
  17-byte record wire format as `scan_chunk`.
- **`engine/src/search.rs` `parity_tests` mod** — 4 new tests run the
  single-threaded `scan_chunk` and the rayon `scan_parallel` over the
  same seed range and assert byte-identical output. Covers a 10k
  partial-voucher filter, a 50k strict-joker filter, a 20k 3-clause
  partial filter, and a 5k offset start range. Uses a shared 4-thread
  pool via `std::sync::Once`.
- `cargo test --release` is **33/33** passing.
- `cargo test --release --features rayon` is **37/37** passing.

### Web (both apps)
- New `pkg-threads/` output vendored to
  `web/public/engine-threads/` and
  `Balatropedia/client/public/engine-v2-threads/`. 314 KB threaded
  wasm, plus the `workerHelpers.js` snippet shipped by
  `wasm-bindgen-rayon` for nested worker spawning.
- New `searchWorkerThreaded.ts` / `seedFinderV2WorkerThreaded.ts`
  loaders. On startup each calls
  `initThreadPool(Math.min(navigator.hardwareConcurrency, 16))` and
  then runs `scan_chunk_parallel` in adaptive 50 k – 8 M batches
  targeting ~400 ms each.
- **`web/src/search/orchestrator.ts`** and
  **`Balatropedia/client/src/lib/seedFinderV2.ts`** — both gained a
  threaded-mode branch. When `crossOriginIsolated === true` and
  `SharedArrayBuffer` is available, the orchestrator spawns ONE
  threaded worker for the full range. Otherwise it falls back to the
  existing N-worker fan-out (capped at 8). New `threaded` engine label
  surfaces in the UI.
- **Cold-start polish.** `App.tsx` and `SeedFinderTab.tsx` each gained
  a prewarm `useEffect` that issues
  `fetch({ cache: 'force-cache' })` + `WebAssembly.compileStreaming`
  for the threaded / SIMD / scalar bundles on mount, so the first
  search after a fresh launch starts ~200–400 ms sooner.
- **Filter + config persistence.** Standalone now persists the last
  filter and engine config to `localStorage`
  (`seed-searcher-last-filter-v1`, `seed-searcher-last-config-v1`).
  Precedence on load: URL hash > localStorage > defaults.

### Honestly deferred
- **Phase 3a — SIMD batch hashing.** Processing 2 seeds per pseudohash
  loop with 128-bit WASM SIMD would save ~5–10 % wall time on the hot
  path; pseudohash is a small fraction of `evaluate()`, the filter ops
  and clones dominate. Not worth a multi-day SIMD lane rewrite.
- **Phase 3b — Seed-space pre-filter LUT.** Full 2^41 LUT is
  impossible. A probabilistic bloom over the first 2 RNG outputs
  would help selective strict-AND clauses but needs careful divergence
  control and is out of scope for this sprint.

### Phone-realistic expectations
- **Phase 1 (rayon threading):** 1.8–2.2× on mid-range phones with 4–8
  cores. Only activates when the host serves COOP `same-origin` +
  COEP `require-corp`. The standalone deploy and the APK can do this;
  Balatropedia’s server sends `crossOriginEmbedderPolicy: false` on
  purpose (to keep cross-origin embedding working) so it transparently
  falls back to N-worker mode there.
- **Phase 2 (cold-start polish):** 200–400 ms saved on the very first
  search after a fresh app launch. No effect on subsequent searches.

### APK (Capacitor) notes
- For threaded mode to activate inside the WebView, the Capacitor host
  must inject `Cross-Origin-Opener-Policy: same-origin` +
  `Cross-Origin-Embedder-Policy: require-corp` on the `index.html`
  response. Typical pattern: subclass
  `WebViewClient.shouldInterceptRequest` and add the headers to the
  `WebResourceResponse`. Without this the engine falls back to
  N-worker (still works, just slower). Wiring this is the next APK
  task and is not part of this release.

## v3.0-beta1 — 2026-06-29 (WebGPU scaffold, search backend unchanged)

First beta of the V3 (WebGPU) engine path. **Searches still run on the
WASM CPU engine.** V3 ships WebGPU detection, a verified diagnostic
compute shader, and the fallback chain that real GPU search will plug
into once the precision blocker is solved. See `docs/V3_DESIGN.md`.

### Engine
- **`engine/src/v3/df.rs`** — Dekker / Knuth double-float arithmetic
  (two-sum, two-product, add/sub/mul/div/floor/fract, i64-halves
  helpers) used for the f32 parity investigation.
- **`engine/src/v3/pseudohash.rs`, `lua_random.rs`, `boss_probe.rs`** —
  DF-emulated mirrors of the hot path. Tests document, but do not
  assert, the divergence from the reference `f64` outputs: DF
  pseudohash drifts 0.3–0.6 from f64 after a few dozen iterations, and
  the DF boss probe agrees with f64 only ~4.3 % of the time. This is
  the data that ruled out the original "f32 + CPU verify" plan; see
  V3_DESIGN.md for the reasoning.
- **`engine/src/v3/diagnostic.rs`** — integer-only tausworthe RNG that
  serves as the verification reference for the WGSL shader.
- **`engine/shaders/diagnostic.wgsl`** — 163-line compute shader,
  validated by `naga`, runs the same tausworthe across 4096 lanes /
  64-thread workgroups. Output verified bit-for-bit against
  `v3_diagnostic_cpu` inside the browser before V3 reports `ready`.
- **`v3_diagnostic_cpu` and `v3_diagnostic_shader_source`** added to
  the `wasm_bindgen` API.
- `cargo test --release` is 33 / 33 passing.

### Web (Balatropedia + standalone)
- New `v3/webgpuEngine.ts` and `v3/engineSelector.ts` modules in both
  apps. `selectEngine({ v3Beta, wasm })` returns an `EngineDescriptor`
  with `searchBackend: 'wasm'` enforced at the type level (V3 cannot
  silently divert searches to an unverified GPU path).
- **V3 beta toggle** wired into both apps, hidden by default. Enable
  via `?v3=1` in the URL; preference persists in `localStorage`.
- When V3 is enabled the app probes WebGPU on mount and shows one of:
  `WebGPU verified · <adapter> · ~XM ops/s diagnostic`,
  `WebGPU unavailable: <reason>`, or
  `WebGPU verification failed: <reason>`.
- Engine indicator gains a `V3 beta` pill in the header (standalone)
  when the toggle is on.

### Known non-issues
- V3 does not change throughput on seed searches in this build. That
  is intentional and documented — see V3_DESIGN.md § "What actually
  happened" for why `f32` and DF emulation can't drop in for the
  pseudohash + LuaRandom chain, and § "Paths forward" for what the
  next sprint would look like.

## v2.1 — 2026-06-29 (perf + responsiveness)

Focused performance and UI-responsiveness pass. No semantic changes to seed
evaluation; parity vs Immolate still bit-for-bit (100k-seed sweep, zero
divergences after refactor).

### Engine
- **`Instance` is now `Clone`**. `Filter::evaluate` builds one template
  `Instance` per seed (paying the `pseudohash` + `LuaRandom` 10-iter
  warmup + `BTreeSet` allocation once) and `.clone()`s it for every
  clause. Previously each clause called `Instance::new(seed_str)`, which
  re-ran the warmup. Roughly N× cheaper for filters with N clauses; on a
  typical 4-clause filter, throughput improves measurably without changing
  any RNG semantics (clone copies all RNG state byte-for-byte).
- **Selectivity-ordered clause execution**. `Filter::compile` now sorts
  ops by `estimated_selectivity()` (Wraith→Rare 6e-4, edition probes
  3e-4, plain joker 3e-2, etc.) so the rarest probe runs first. Strict-AND
  short-circuits aggressively; partial mode is unaffected because it
  always scores all clauses.
- All 21 Rust unit tests still pass; bit-for-bit parity sweep clean across
  100k seeds (determinism + scan↔inspect + interleave stability).

### UI (Balatropedia + standalone)
- **Counters tick from click time, not after a 3-second delay**. Worker
  initial batch size dropped from 200k → 5k seeds so the first
  `scan_chunk` returns in <10ms even on slow CPUs. Worker emits an
  alive-ping immediately on receiving the scan message, before WASM is
  even loaded. Orchestrator emits a synchronous progress event on
  `start()` and ticks every 100ms (down from 200–250ms).
- **Phase-aware progress display** (Balatropedia). Progress events now
  carry a `phase` field (`"loading"` / `"warming"` / `"running"`) so the
  UI can show "loading WASM…" / "warming up…" / actual rate, instead of
  a misleading "0 seeds/s" while bytes are still being fetched.

## v2 final — 2026-06-29

V2 is out of beta. The Rust+WASM engine is the default seed search backend
on Balatropedia and the standalone web app.

### Engine
- **`inspect_seed(filter_json, seed, deck_idx, stake_idx)`** — new wasm
  export. Returns `{ok, seed, clauses[{index, matched, detail}], matched,
  total}` for any single seed. Powers the Verify Seed inspector in the UI
  and the bit-for-bit parity harness.
- All 21 Rust unit tests pass against the rewritten `wasm_api.rs`. Scalar,
  SIMD, and node targets all expose the new entry point.
- Standard pack card matching extended: optional independent `suit` and
  `rank` filters in addition to the canonical base name (`"Ace of Spades"`).
- Enhancement names accept both short ("Glass") and long
  ("Glass Card") forms.
- Seal names accept both short ("red") and long ("Red Seal") forms.

### Parity
- New `scripts/parity_bitwise.js` runs determinism, scan↔inspect
  consistency, and interleave-stability sweeps. Default 100k seeds, scales
  to 2M. Latest run: zero divergences across all phases. See
  `scripts/parity_bitwise_results.json`.
- Statistical parity vs Immolate still tight (tags 7.6M seeds,
  Δ = 2.17e-4). `scripts/parity_harness.js` unchanged.

### UI (Balatropedia + standalone)
- Beta toggle and "BETA" labels removed. V2 is default. V1 engine remains
  available via `?legacy=1` URL flag for diagnostics only.
- First-class filter rows for every V2 clause family:
  - Joker (with edition + sticker dropdowns and a Source selector that
    routes shop / buffoon-pack / arcana-soul / spectral-soul /
    spectral-wraith / legendary-soul).
  - Voucher (per-ante).
  - Tag (per-ante, with small-blind / big-blind position selector).
  - Boss (per-ante, dropdown of all 28 bosses).
  - Standard pack card (suit, rank, enhancement, edition, seal —
    all independently optional).
- "Add filter" popover surfaces the four non-joker clause families.
- Legendary jokers (Canio, Triboulet, Yorick, Chicot, Perkeo) auto-route
  through the engine's Soul resolver. Rare-via-Wraith picker available
  when source = spectral-wraith.
- **Verify seed** inspector — paste any seed, see which clauses matched
  and the human-readable detail per clause ("ante 1 · shop slot 3
  [Negative]", "ante 2 boss = The Wall (wanted The Mark)", etc.).
- **Share URL roundtrip** — every filter config encodes to a base64url
  blob in `?seedfinder=...`. Opening a shared URL restores the full
  filter set.
- Worker pool size control retains Eco → Extreme tiers with a
  per-device recommendation; matches `hardwareConcurrency` and survives
  page reload via `localStorage`.

### Performance notes
- Scalar WASM bundle: 230 KB (gzipped ~95 KB).
- SIMD WASM bundle: 230 KB (gzipped ~95 KB); same surface area, browsers
  that report SIMD support load it automatically and the per-worker
  throughput tag reports "SIMD" / "scalar" / "mixed".
- On a 2 vCPU sandbox: scan_range throughput ranges from 16k seeds/sec
  (combined 6-clause filter) to 220k seeds/sec (single tag clause).

## v2.1 beta — 2026-06-26

Initial public V2 preview. Tag parity 7.6M seeds vs Immolate, statistical
parity for all other clause families, no bit-for-bit harness yet.

## v1 — pre-2026

JS engine (Immolate-based). Now legacy-fallback only via `?legacy=1`.
