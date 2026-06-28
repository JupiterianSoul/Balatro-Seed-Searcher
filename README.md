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
implementation. But it's also a single-session greenfield build, so
some things are not yet at parity with Immolate:

1. **Boss lock-pool reopening** is approximated (ante-keyed cache node).
   Exact for single-ante queries, close-enough for multi-ante. Real
   modelling requires tracking `locked[]` per instance — landing next.
2. **Joker resample on locks** (showman / restock pool) is skipped.
   The first roll is exact; resamples will need the lock state machine.
3. **Pack contents** are not yet simulated — the bytecode accepts the
   clause and the search loop just returns false for it, so users get
   a clean "no match" rather than a wrong match.
4. **Sticker draws** (eternal/perishable/rental) are unimplemented.
5. **Cross-validation against Immolate** is per-test (deterministic +
   variety + boss-pool); full bit-for-bit parity sweep across 10k+
   seeds lands in milestone 2.

The goal of this project is to be **honest about what's working and
what isn't**, while building toward the fastest correct browser seed
finder. If you spot a determinism mismatch with the game, please open
an issue with the seed + filter + observed vs expected.

## License

MIT. See [LICENSE](LICENSE).

## Credits

- RNG math + source-key naming: ported from
  [MathIsFun0/Immolate](https://github.com/MathIsFun0/Immolate) (also MIT).
- Balatro is © LocalThunk. This project does not include any Balatro
  source code or assets — only the deterministic seed-derivation math
  the community has documented publicly.
