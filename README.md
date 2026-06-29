# Balatro Seed Searcher

Balatro Seed Searcher is a from scratch search engine for Balatro seeds. The core is written in Rust, compiled to WebAssembly and run inside Web Workers, so the whole thing executes in the browser with no install, no server round trip and no telemetry. It powers the Finder tab inside Balatropedia and also ships a small standalone web demo for benchmarking.

## What it does

You describe a board you want (a specific joker with a sticker, a specific voucher in a specific ante, a tag attached to a small blind, a standard pack with a glass king of hearts, and so on), the engine then enumerates seeds and reports every match. The same engine can take a single known seed and walk the simulation forward to show shops, packs, vouchers and Soul resolutions ante by ante.

## How it works

The engine is a faithful port of Balatro's own random number sequence. Three pieces drive it:

* A pseudohash that mixes the seed string into a stable 64 bit state. This is the same procedure the game uses to turn a typed seed into a starting RNG state.
* A linear congruential generator (LCG) that advances that state every time the game asks for a random number. The constants match the game one to one, which is why the search results line up bit for bit with what you would actually see in a real run.
* A derivation tree built on top of the RNG that mirrors the game's lookup order: ante setup, then shops (jokers, vouchers, tags), then booster packs (buffoon, arcana, celestial, spectral, standard), then Soul and Wraith resolutions. Each node caches its result so re-deriving inside one ante is free.

On top of that the engine compiles your filter into a small bytecode. Cheap clauses (boss, voucher, tag) run first; expensive ones (specific edition plus sticker on a specific joker) run last. The orchestrator hands out seed ranges to Web Workers, each worker has its own WASM instance, and matches stream back as they appear. SIMD is detected at startup; if the browser supports it the SIMD bundle loads, otherwise the scalar bundle does. Both produce the same results.

The 100k seed regression sweep (`cargo run --release --bin verify -- 100000`) re-derives a known reference set and exits non-zero on any divergence. The score so far is zero divergences across all checked sweeps.

## Filter surface

You can stack any combination of:

* Joker constraint with edition (none, foil, holographic, polychrome, negative), sticker (none, eternal, perishable, rental) and source (shop, buffoon pack, arcana Soul, spectral Soul, spectral Wraith, legendary Soul)
* Voucher constraint pinned to a specific ante
* Tag constraint with small blind or big blind position
* Boss constraint covering all 28 bosses
* Standard pack card constraint with independent suit, rank, enhancement, edition and seal fields
* Soul resolved to a specific legendary (Canio, Triboulet, Yorick, Chicot, Perkeo)
* Wraith resolved to a specific Rare joker

`AnyOf` is a first class clause, so you can ask for "Blueprint OR Brainstorm" without splitting the search.

## Codes used

The constants and tables in the engine are not invented. They are read from the game's own data:

* RNG constants (LCG multiplier, increment, modulus) match the values used by `pseudohash` and `pseudorandom` in the game
* Joker, voucher, tag and boss tables are synced to the canonical display names from Immolate's `lib/items.cl`. A helper binary, `extract_canonical_names`, regenerates them when the upstream table changes.
* Pack composition, shop slot counts and rerolls per ante follow the published Balatro rules. The constants live in `engine/src/tables.rs`.
* Soul and Wraith resolution branches follow the same RNG calls the game makes when those cards trigger.

## Performance

* Worker pool sized by a device profiler that reads core count, mobile vs desktop and reported RAM. Defaults from Eco (2 workers, low end phones) up to Extreme (24+ cores).
* Adaptive batch size per worker. Smaller batches on phones to keep the UI thread responsive, larger batches on desktops to amortise WASM dispatch.
* Node cache inside one ante so the same shop slot is never re-derived if two clauses touch it.
* Filter compiler re-orders clauses by selectivity at compile time.

## Repo layout

| Folder | What it is |
|---|---|
| `engine/` | The Rust crate: RNG, cache, derivations, filter compiler, search loop, WASM bindings |
| `engine/src/tables.rs` | Canonical joker, voucher, tag and boss tables |
| `engine/src/bin/` | `bench_throughput`, `verify` (regression sweep), `smoke_anyof` (AnyOf sanity), `extract_canonical_names` (sync helper) |
| `web/` | Standalone React and Vite UI, used as a demo and a benchmark harness |
| `scripts/build-wasm.sh` | Builds both scalar and SIMD WASM bundles and copies them into `web/public/engine/` and `web/public/engine-simd/` |

## Build and run

```bash
# Engine: unit tests plus regression sweep
cd engine
cargo test --release
cargo run --release --bin verify -- 100000

# WASM bundles (scalar plus SIMD)
./scripts/build-wasm.sh

# Standalone web UI
cd ../web && npm install && npm run dev
```

## Sharing filters

The standalone UI and the Balatropedia Finder both serialise the current filter into a `seedfinder` query parameter (base64url encoded). Open the URL on another device and the same filter is loaded.

## License

See `LICENSE`. The engine is independent code; it does not include or redistribute any Balatro asset. Display names from Immolate's tables are kept in sync but not bundled as game data.
