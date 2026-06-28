# Balatro Seed Searcher

A from-scratch Balatro seed search engine — Rust core, WebAssembly delivery
(scalar + SIMD), parallelised across Web Workers. Runs entirely in the
browser; no install, no server round-trip, no telemetry.

**Live demo (standalone):** deployed via the Perplexity asset-share URL in
`/web/`. **Live demo (integrated):** wired into
[Balatropedia](https://github.com/JupiterianSoul/Balatropedia) behind a beta
toggle in the Seed Finder tab.

```
   ┌───────────────────────────────────────────────────────────┐
   │   Browser (no install required)                           │
   │                                                           │
   │   ┌──────────────┐    ┌──────────────────────────────┐    │
   │   │ React UI     │ ←→ │ Orchestrator (main thread)   │    │
   │   │ • Filter     │    │ • partitions seed space      │    │
   │   │   builder    │    │ • aggregates matches         │    │
   │   │ • Streaming  │    │ • progress + engine status   │    │
   │   │   results    │    └──────────┬───────────────────┘    │
   │   │ • Device     │               │                        │
   │   │   profiler   │     ┌─────────┴─────────┐              │
   │   └──────────────┘     │ N Web Workers     │              │
   │                        │ • Rust → WASM     │              │
   │                        │ • SIMD detection  │              │
   │                        │ • adaptive batch  │              │
   │                        └─────────┬─────────┘              │
   │                                  ▼                        │
   │                        ┌───────────────────┐              │
   │                        │ WASM engine       │              │
   │                        │ pseudohash + LCG  │              │
   │                        │ + node cache      │              │
   │                        │ + filter bytecode │              │
   │                        └───────────────────┘              │
   └───────────────────────────────────────────────────────────┘
```

## Why a new engine?

Existing browser-based Balatro seed finders (including the one originally in
[Balatropedia](https://github.com/JupiterianSoul/Balatropedia)) embed prebuilt
[Immolate](https://github.com/SpectralPack/Immolate) WASM artefacts. That
works, but means you cannot:

- Rebuild with SIMD on demand (no source in the dependent repos)
- Re-order filter clauses by selectivity
- Express disjunctions (`AnyOf`) cleanly in the DSL
- Tune batch sizes, threading, and worker pacing per device class

This project owns the engine end-to-end so all of the above is possible.

## What's in this repo

| Folder | What it is |
|---|---|
| `engine/` | Rust crate — RNG, cache, derivations, filter compiler, search loop, WASM bindings |
| `engine/src/tables.rs` | Canonical item tables — joker pools, tags, vouchers, bosses, packs (synced to Immolate `lib/items.cl` display names) |
| `engine/src/bin/` | `bench_throughput`, `verify` (100k regression sweep), `smoke_anyof` (AnyOf hit-rate sanity), `extract_canonical_names` (sync helper) |
| `web/` | Standalone React + Vite UI — minimal demo of the engine, used for benchmarking and SIMD verification |
| `scripts/build-wasm.sh` | Builds scalar + SIMD WASM bundles, copies them into `web/public/engine{,-simd}/` |

## Quick start

```bash
# 1. Engine — run all tests and the regression sweep
cd engine
cargo test --release            # 21/21 unit + integration tests
cargo run --release --bin verify -- 100000

# 2. Build both WASM bundles (scalar + SIMD)
./scripts/build-wasm.sh
# → engine/pkg/, engine/pkg-simd/
# → also copies into web/public/engine{,-simd}/

# 3. Standalone web UI
cd ../web && npm install && npm run dev
```

To use the engine from another project (e.g. Balatropedia), copy
`engine/pkg/` and `engine/pkg-simd/` into the host's `public/`, and load both
in a Web Worker — the worker probes `WebAssembly.validate` with a SIMD
sentinel and picks whichever bundle works.

## Filter DSL

JSON-serialisable, designed to be hand-writable but also UI-buildable.
Strict mode requires every top-level clause to match; partial mode ranks
results by how many clauses match.

```json
{
  "clauses": [
    { "kind": "ante_shop_has_joker", "ante": 1, "slot": 0, "joker": "Blueprint" },
    { "kind": "voucher_is", "ante": 2, "voucher": "Telescope" },
    { "kind": "any_of", "clauses": [
      { "kind": "ante_tag_is", "ante": 1, "position": 0, "tag": "Negative Tag" },
      { "kind": "ante_tag_is", "ante": 2, "position": 0, "tag": "Negative Tag" }
    ]}
  ],
  "partial": false,
  "min_score": null
}
```

Supported clauses:

- `ante_shop_has_joker { ante, slot, joker, edition? }` — slot 0 only at the
  moment (see Known gaps).
- `ante_tag_is { ante, position, tag }`
- `ante_boss_is { ante, boss }`
- `voucher_is { ante, voucher }`
- `ante_pack_contains { ante, pack_index, card }` — pack types `Arcana`,
  `Spectral`, `Celestial`, `Buffoon`. Standard packs return contents but
  individual card draws (rank/suit/seal/enhancement) are not yet modelled.
- `any_of { clauses: [...] }` — disjunction. Sub-clauses share `Instance`
  state, so checking the same joker across antes 1..8 advances the shop
  draw sequence naturally and is cached per `(kind, source, ante)`.

## Engine status

- **Native single-thread:** ~570k seeds/s on a typical joker filter,
  ~1M seeds/s when voucher-only short-circuits hit first.
- **WASM SIMD vs scalar:** SIMD bundle wins on every Chromium/Edge run,
  loses on Safari (no SIMD support yet) — the worker detects and falls back.
- **Tests:** 21/21 unit + integration green.
- **Regression sweep (`verify -- 100000`):** 0 determinism failures, 0 pool
  misses, 0 ante-1 locked-boss leaks, 0 white-stake sticker leaks. Joker
  rarity at ante-1 shop: common 70.22 % / uncommon 24.78 % / rare 5.00 %.
  Pack weights within 0.05 % of Immolate's PACKS table.

## Honest gaps

1. **Multi-slot shop scan** — `Op::HasJoker` currently only checks shop
   slot 0. Most user constraints are "X appears in ante N at all" so the
   shop is iterated via `any_of` over antes, not slots; this means a joker
   that exists only in slot 1+ but never slot 0 of any ante won't match.
   Tracked.
2. **Standard pack card-level modelling** — `open_pack` returns pack-level
   contents (the slot exists, the kind is correct) but individual playing
   cards inside Standard packs don't get rank/suit/seal/enhancement draws
   yet.
3. **Bit-for-bit Immolate cross-validation** — the 100k `verify` sweep
   proves determinism, pool validity, lock correctness, sticker correctness,
   and distribution match. It does **not** yet prove byte-identical match
   with Immolate. That requires running Immolate's OpenCL binary on the
   same 100k seeds and diffing. Tracked.
4. **Canonical-name drift** — fixed 2026-06-29. Earlier builds of the
   engine had stripped punctuation in item tables (`Mr Bones` instead of
   `Mr. Bones`, `Riff raff` instead of `Riff-raff`, `Drivers License`
   instead of `Driver's License`) and a swap of `Stuntman`/`Vampire`
   between Uncommon and Rare. Both pools and the
   `ETERNAL_BLACKLIST`/`PERISHABLE_BLACKLIST` checks were inconsistent
   with each other, so sticker filtering on `Mr. Bones` silently failed
   and any client-side filter matching by display name (e.g. Balatropedia)
   returned zero hits for the affected jokers. Now strictly synced to
   Immolate's `lib/debug.cl` display strings.

## Cross-validation against Immolate

The item tables in `engine/src/tables.rs` follow the exact order and
spelling of `J_C_BEGIN..J_L_END` in
[Immolate's `lib/items.cl`](https://github.com/SpectralPack/Immolate/blob/main/lib/items.cl),
with display strings taken from `lib/debug.cl`. If you spot drift, the
helper script in `engine/src/bin/extract_canonical_names.rs` re-diffs both
sides.

## License

MIT. See [LICENSE](LICENSE).

## Credits

- RNG math + source-key naming: ported from
  [SpectralPack/Immolate](https://github.com/SpectralPack/Immolate) (MIT).
- Balatro is © LocalThunk. This project does not include any Balatro
  source code or assets — only the deterministic seed-derivation math
  the community has documented publicly.
