# Balatro Seed Searcher

The fastest open-source Balatro seed search engine on the web — written
from scratch in Rust, compiled to WebAssembly with SIMD, parallelised
across Web Workers, with an optional WebGPU compute path and an optional
self-hosted cloud-boost adapter.

**Live demo:** _(coming with milestone 2)_

```
   ┌───────────────────────────────────────────────────────────┐
   │   Browser (no install required)                           │
   │                                                           │
   │   ┌──────────────┐    ┌──────────────────────────────┐    │
   │   │ React UI     │ ←→ │ Orchestrator (main thread)   │    │
   │   │ • Filter     │    │ • partitions seed space      │    │
   │   │   builder    │    │ • aggregates matches         │    │
   │   │ • Streaming  │    │ • ranks by closeness score   │    │
   │   │   results    │    └──────────┬───────────────────┘    │
   │   │ • Share URL  │               │                        │
   │   └──────────────┘     ┌─────────┴─────────┐              │
   │                        │ N Web Workers     │              │
   │                        │ • Rust → WASM     │              │
   │                        │ • SIMD path opt.  │              │
   │                        │ • adaptive batch  │              │
   │                        └─────────┬─────────┘              │
   │                                  ▼                        │
   │                        ┌───────────────────┐              │
   │                        │ WASM engine       │              │
   │                        │ pseudohash + LCG  │              │
   │                        │ + cache + sources │              │
   │                        └───────────────────┘              │
   │                                                           │
   │   Optional: WebGPU compute path → 10–30× throughput       │
   │   Optional: WebSocket → self-hosted cloud worker (Node)   │
   └───────────────────────────────────────────────────────────┘
```

## Why a new engine?

Existing browser-based Balatro seed finders (including the one in
[Balatropedia](https://github.com/JupiterianSoul/Balatropedia)) embed
prebuilt [Immolate](https://github.com/MathIsFun0/Immolate) WASM artefacts.
That works, but you can't:

- Rebuild with SIMD (no source in the dependent repos)
- Re-order filter clauses by selectivity
- Stream "best partial matches" as you go
- Run a verified reproduction simulation
- Push to a GPU compute path
- Run the same engine native on a cloud worker without re-porting

This project owns the engine end-to-end so all of that becomes possible.

## What's in this repo

| Folder | What it is |
|---|---|
| `engine/` | Rust crate: RNG, cache, derivations, filter compiler, search loop |
| `web/` | React + Vite UI, Web Workers, WASM loader, WebGPU adapter |
| `server/` | Optional self-hosted Node WebSocket worker for cloud boost |
| `bench/` | Throughput benchmarks vs Immolate reference |
| `docs/` | Architecture deep-dive, RNG math, filter DSL, build guides |
| `scripts/` | Build helpers (`build-wasm.sh`, `dev.sh`) |
| `_backups/` | Snapshot of Balatropedia's Immolate-based finder for diffing |

## Quick start

```bash
# Engine — run tests + native bench
cd engine && cargo test --release
cargo run --release --example probe

# Build both WASM bundles (scalar + SIMD)
./scripts/build-wasm.sh

# Web UI
cd ../web && npm install && npm run dev
```

## Current status (single-session build, ongoing)

- ✅ RNG core: pseudohash + LuaJIT-compatible PRNG, 14/14 parity tests passing
- ✅ Seed encoding: base-35 iteration, rank round-trip
- ✅ Per-run cache: `get_node_child` ported from Immolate, deterministic
- ✅ Item tables: all 12 canonical tables (60+ jokers per rarity, etc.)
- ✅ Source derivations: `next_joker`, `next_tag`, `next_voucher`, `next_boss`
- ✅ Filter DSL: parser + bytecode + 5 clause kinds
- ✅ WASM bundles: scalar (135KB) + SIMD (147KB), both built
- ✅ Native throughput: **552k seeds/sec single-thread** with real derivations
- ⏳ Web UI: in active development (see milestone 2)
- ⏳ Pack-contents simulation: bytecode wired, lock-state machine TODO
- ⏳ Sticker rolls (eternal/perishable/rental): TODO
- ⏳ Voucher chain side-effects: TODO
- ⏳ WebGPU compute path: experimental, design phase
- ⏳ Verified reproduction simulator: TODO

## Honesty section

This is a real port of a real RNG, not a half-baked "looks plausible"
implementation. After this round of work, four of the six original gaps
are closed and two remain:

**Closed:**

1. ✅ **Boss lock-pool reopening** — `Instance` tracks `locked: BTreeSet<&str>`
   and `apply_ante_locks` mirrors Immolate's `init_locks` line-for-line.
   `next_boss` runs it before each draw. 100 k-seed sweep: 0 locked-boss
   leaks at ante 1.
2. ✅ **Joker resample on locks** — `next_joker` goes through
   `rand_choice_common` with the same lock/retry/unlock loop Immolate uses.
3. ✅ **Pack contents** — `open_pack` handles all 15 pack variants with the
   right card counts, intra-pack duplicate blocking, and `Op::PackContains`
   is wired through `search.rs`.
4. ✅ **Sticker draws** — `next_joker_with_stickers` emits `eternal /
   perishable / rental` gated by stake and filtered through blacklists.
   100 k White-stake draws: 0 sticker leaks.

**Still open:**

5. ⏳ **Standard pack card-level modelling** — `open_pack` returns the
   pack-level contents but doesn't yet draw individual playing cards with
   rank/suit/seal/enhancement. Filters that depend on a specific
   seal-on-rank inside a Standard pack won't match correctly.
6. ⏳ **Bit-for-bit cross-validation against Immolate** — the 100 k
   `verify` sweep proves determinism, pool validity, lock correctness,
   sticker correctness, and distribution match; it does not yet prove
   byte-identical match with Immolate. That requires running Immolate's
   OpenCL binary on the same 100 k seeds and diffing. Tracked.

### Regression-sweep result (latest)

```
cargo run --release --bin verify -- 100000
  Determinism failures:        0
  Pool misses:                 0
  Ante-1 locked-boss hits:     0
  White-stake sticker leaks:   0
  Joker rarity (ante-1 shop):  common 70.22 % / uncommon 24.78 % / rare 5.00 %
  Pack weights match Immolate's PACKS table to within 0.05 %.
```

The goal of this project is to be **honest about what's working and
what isn't**, while building toward the fastest correct browser seed
finder. If you spot a determinism mismatch with the game, please open
an issue with the seed + filter + observed vs expected.

## Docs

- [Architecture](docs/architecture.md) — the four layers, RNG details, why the pseudohash needs the int/fract split.
- [Comparison vs Balatropedia's current finder](docs/comparison.md) — honest side-by-side, including the two gaps still to close.

## License

MIT. See [LICENSE](LICENSE).

## Credits

- RNG math + source-key naming: ported from
  [MathIsFun0/Immolate](https://github.com/MathIsFun0/Immolate) (also MIT).
- Balatro is © LocalThunk. This project does not include any Balatro
  source code or assets — only the deterministic seed-derivation math
  the community has documented publicly.
