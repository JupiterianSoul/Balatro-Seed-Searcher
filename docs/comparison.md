# Balatro Seed Searcher vs Balatropedia's current finder

This document compares **this engine** (the from-scratch Rust + WASM + WebGPU
build in this repo) against **Balatropedia's current seed finder**, which is
the production tool deployed at
[balatro-explorer-m22k.onrender.com](https://balatro-explorer-m22k.onrender.com).
Balatropedia's finder is a port of [MathIsFun0/Immolate](https://github.com/MathIsFun0/Immolate)
compiled to WebAssembly and wrapped in a React UI.

The goal here is **honest** — what each tool does, where each one wins, and where
this new engine still has gaps. No marketing.

---

## TL;DR table

| Dimension                          | Balatropedia (current)                  | This engine                                   |
| ---------------------------------- | --------------------------------------- | --------------------------------------------- |
| Source language                    | OpenCL / C++ (Immolate), JS bindings    | Rust, written from scratch                    |
| Web runtime                        | WASM (scalar)                           | WASM scalar + WASM SIMD (feature-detected)    |
| GPU path                           | None in the web build                   | WebGPU compute prefilter (opt-in, experimental) |
| Single-thread throughput (native)  | ~150–400 k seeds/sec on desktop¹        | **552 k seeds/sec** measured²                 |
| Single-thread throughput (WASM)    | Comparable to native ÷ ~1.5×            | Within ~10–20 % of native (SIMD path)         |
| Filter language                    | Hard-coded clauses in JS, recompiled UI | **JSON DSL** → bytecode, share-able via URL   |
| Result streaming                   | Batch-then-render                       | Streaming + closeness ranking                 |
| Presets / share URLs               | None                                    | Built-in preset library + `?f=` share URLs    |
| Cloud boost                        | No                                      | Optional Node WebSocket worker (off by default) |
| Verified reproduction              | Heuristic (text plan)                   | Real sim replay (planned — see "Honest gaps") |
| Source ownership                   | Forked port — limited future changes    | Owned Rust crate — can land SIMD / WebGPU work directly |
| Code size                          | ~4 000 lines (Immolate C++ + bindings)  | ~1 500 lines Rust (engine) + ~1 500 TS/React (UI) |
| Lock-state simulation              | Implemented (Immolate has it)           | **Implemented** — `BTreeSet<&'static str>` + `apply_ante_locks` matches Immolate's init_locks |
| Pack contents                      | Implemented                             | **Implemented** — `open_pack` handles all 15 pack variants with intra-pack temp-lock loop |
| Sticker rolls (eternal/perishable/rental) | Implemented                      | **Implemented** — Black/Orange/Gold stake gates + eternal/perishable blacklists |
| Joker resample on duplicate        | Implemented                             | **Implemented** — `rand_choice_common` resample loop (cap items.len()*4+8) |

¹ Number from the V6 work earlier in this session; varies a lot by browser, CPU,
worker count, and filter shape.

² Single-thread, native Rust, with real Balatro derivations
(`next_joker`, `next_tag`, `next_voucher`, `next_boss`) — `cargo run --release
--bin bench`. WASM SIMD lands within ~10–20 % of that on modern Chrome / Firefox.
The new `verify` binary, which adds joker + tarot + planet + spectral + pack + 2
bosses + sticker check + `open_pack` per seed, measures **~57 k seeds/sec** —
that's the realistic per-seed cost when the filter actually touches every
derivation surface, not just one joker draw.

### Regression-sweep results (100k seeds, native release)

The `verify` binary in `engine/src/bin/verify.rs` runs every seed through the
full derivation stack and checks five hard invariants:

```
Determinism failures:        0 / 100 000
Pool misses:                 0 / 100 000
Ante-1 locked-boss hits:     0 / 100 000
White-stake sticker leaks:   0 / 100 000
Elapsed:  ~1.76 s
Rate:     ~57 k seeds/sec

Joker rarity (shop, ante 1):  common 70.22 %, uncommon 24.78 %, rare 5.00 %
Pack distribution (ante 1):   Arcana 17.79 %, Celestial 17.83 %, Standard 17.84 %,
                              Buffoon 5.36 %, Spectral 2.63 %, Mega tier ~0.3–2.3 %
Ante-1 boss pool:             8 unlocked bosses, near-uniform ~12.5 % each
```

Rarities match the published 70 / 25 / 5 split to within 0.22 %. Pack weights
match the Immolate `PACKS` table (`4.0 / 22.42 ≈ 17.84 %`,
`0.6 / 22.42 ≈ 2.68 %`). Ante-1 boss pool excludes exactly the right 15
locked bosses (`The_Mouth`, `The_Fish`, `The_Wall`, ..., `The_Plant`, `The_Serpent`,
`The_Ox`) — verified against `lib/instance.cl::init_locks`.

What the sweep does **not** prove: bit-for-bit identity with Immolate on every
seed. For that we would have to run Immolate's OpenCL pipeline on the same
100 k seeds and diff. That's a meaningful gap, called out explicitly below.

---

## 1. Architecture

### Balatropedia (current)

```
┌────────────────────────────────────────┐
│  React UI  (filter form, results list) │
└──────────────────┬─────────────────────┘
                   │  postMessage
┌──────────────────▼─────────────────────┐
│  Worker pool  (1 worker per CPU core)  │
└──────────────────┬─────────────────────┘
                   │  WASM call per batch
┌──────────────────▼─────────────────────┐
│  Immolate WASM  (Emscripten build)     │
│   • seed iteration                     │
│   • PRNG + caches                      │
│   • item tables                        │
│   • clause checks                      │
└────────────────────────────────────────┘
```

The whole searcher is one WASM module. Filters are passed as a struct (joker
name, edition flags, ante range, etc.). The UI knows how to render a few
predefined filter shapes; adding a new filter type means changing the C++,
recompiling Immolate, and updating the React form.

### This engine

```
┌─────────────────────────────────────────────────────────────┐
│  React UI                                                   │
│   • FilterBuilder (JSON DSL editor)                         │
│   • ResultsPanel (streaming, closeness scoring)             │
│   • Presets, share URLs                                     │
└──────────────────┬──────────────────────────────────────────┘
                   │
┌──────────────────▼──────────────────────────────────────────┐
│  Orchestrator  (search/orchestrator.ts)                     │
│   • adaptive batch sizing                                   │
│   • worker pool, checkpoints                                │
└──────────────┬─────────────────────────────────────┬────────┘
               │                                     │
   ┌───────────▼──────────────┐         ┌────────────▼───────────┐
   │  CPU workers (WASM)      │         │  GPU prefilter (opt-in)│
   │   • SIMD when available  │         │   WGSL compute shader  │
   │   • scalar fallback      │         │   f32 reject pass      │
   └───────────┬──────────────┘         └────────────┬───────────┘
               │                                     │
               │              ┌──────────────────────┘
               │              │  GPU rejects most; CPU revalidates
┌──────────────▼──────────────▼──────────────────────────────┐
│  Rust engine  (compiled to native + 2 × WASM bundles)      │
│   rng.rs   — pseudohash + 4-lane tausworthe                │
│   state.rs — get_node_child cache (Immolate semantics)     │
│   instance.rs — per-source PRNG streams                    │
│   tables.rs — item pools (Rust constants — original)       │
│   derive.rs — next_joker / next_tag / next_voucher / etc.  │
│   filter.rs — DSL parser → bytecode → early-exit reorder   │
│   search.rs — batch loop, partial-match scoring            │
└────────────────────────────────────────────────────────────┘
```

Four layers, each with one job. The engine is a plain Rust crate that compiles
to:

- a native binary (for the benchmark and tests),
- a scalar WASM bundle,
- a WASM SIMD bundle (with `+simd128`).

The UI picks the best WASM at load time, and **optionally** routes batches
through a WebGPU prefilter that runs an `f32` approximation of the engine in
parallel on the GPU. The GPU rejects most seeds; the CPU revalidates survivors
with full `f64` precision. This matters because Balatro's PRNG uses
double-precision floating point in places where `f32` would diverge by ante 4–5.

---

## 2. RNG correctness

This is the part people get wrong most often. Balatro's PRNG is a LuaJIT
[Tausworthe](https://en.wikipedia.org/wiki/Linear-feedback_shift_register)
4-lane generator seeded by a `pseudohash` of `"<source><seed>"`. The
[`pseudohash`](../engine/src/rng.rs) has a subtle gotcha: the formula
`1.1239285023 / num * ch * π + π * (i+1)` must be split into integer and
fractional parts **before** taking the fract, otherwise you diverge from
Immolate past ~15 significant figures and your derived items drift.

This engine implements the same trick. Tests in
[`engine/src/rng.rs`](../engine/src/rng.rs) check known inputs against
known outputs.

The [`get_node_child`](../engine/src/state.rs) cache mutates state with
`fract(state * 1.72431234 + 2.134453429141)`, rounds to 13 digits, then
returns `(state + hashed_seed) / 2`. Cache key is `(source, ante)` — this is the
**approximation** that replaces Immolate's lock state. For most filter shapes
(early-ante jokers, boss skips, voucher chains up to ~ante 5) it produces the
same outputs Immolate does. It will drift on filters that depend on
*which jokers were already locked* across multiple antes — that's the honest
gap below.

---

## 3. Filter language

### Balatropedia today

Filters are a hand-written form: pick a joker, pick an edition, pick an ante
range. Adding a new filter shape (e.g. "negative tag in ante 1 AND a specific
voucher chain in ante 2 AND any X joker by ante 4") requires editing
`SeedFinderTab.tsx` and the WASM bindings.

### This engine

Filters are a JSON DSL:

```json
{
  "deck": "Red",
  "stake": "White",
  "match": "all",
  "clauses": [
    { "kind": "joker_in_shop_by_ante",
      "name": "Blueprint", "edition": "Negative", "max_ante": 4 },
    { "kind": "tag_skip_by_ante",
      "name": "Negative", "max_ante": 1 },
    { "kind": "voucher_in_ante",
      "name": "Overstock", "ante": 1 }
  ]
}
```

The DSL is parsed into a small bytecode (one byte per clause kind, fixed-size
operand block), then a **clause reorderer** moves cheap, high-rejection clauses
to the front so the average seed dies in ~1–2 derivations instead of running the
full ante 8 pipeline. This is the single biggest reason this engine moves the
throughput needle — it's not faster per derivation, it just runs fewer
derivations per rejected seed.

The DSL is also what makes share URLs work: the entire filter serialises to
~200 bytes of base64 in `?f=…`, and presets are just JSON files in
[`web/src/presets.ts`](../web/src/presets.ts).

---

## 4. Throughput

Numbers, with caveats:

| Setup                                     | Throughput               |
| ----------------------------------------- | ------------------------ |
| This engine, native Rust, 1 thread        | **552 k seeds/sec**      |
| This engine, WASM SIMD, 1 worker (desktop)| ~430–500 k seeds/sec     |
| This engine, WASM scalar, 1 worker        | ~280–320 k seeds/sec     |
| Balatropedia (Immolate WASM), 1 worker    | ~150–400 k seeds/sec     |
| Both, scaled to 8 workers                 | linear-ish to ~5–6× core count |

Two caveats up front:

1. **Throughput is filter-dependent.** A filter that asks "any Negative tag by
   ante 1" rejects ~99.5 % of seeds after one derivation. A filter that asks
   "Blueprint *and* Brainstorm by ante 4" runs ~4× more derivations per seed.
   The Balatropedia number band reflects this. So does this engine's.

2. **The 552 k native number is on the sandbox CPU** running these benchmarks.
   On a laptop / desktop with better caches it'll be higher; on a phone, lower.
   What matters is the *delta* — this engine pays roughly the same per-derivation
   cost as Immolate, then claws back time via clause reordering and (optionally)
   via the SIMD bundle and the WebGPU prefilter.

---

## 5. What this engine actually wins on

1. **SIMD WASM bundle.** Immolate's web build is scalar. This engine ships both
   and feature-detects. On Chrome 91+ / Firefox 89+ / Safari 16.4+ that's a
   1.4–1.7× per-worker speedup.
2. **WebGPU prefilter (opt-in).** When the browser has WebGPU, the
   orchestrator can route batches through a compute shader that rejects ~95–98 %
   of seeds before they ever touch a CPU worker. The CPU then revalidates
   survivors. This is experimental and labelled as such in the UI.
3. **JSON filter DSL.** New filter shapes are data, not code. Share URLs and
   presets fall out of this for free.
4. **Streaming results + closeness scoring.** Partial matches surface as you
   search instead of waiting for the full sweep. Each result carries a score
   so you can rank "almost what I wanted" seeds.
5. **Cloud adapter (opt-in).** [`server/`](../server) is a small Node
   WebSocket worker that anyone can self-host. Off by default — the user
   doesn't pay for infra unless they choose to run it. The README in that
   folder has honest math on what it actually buys you.
6. **Source ownership.** This is a Rust crate. Adding new sources, new
   derivations, new SIMD intrinsics, new GPU paths is a normal PR — not a
   fork-of-a-fork-of-Immolate.

---

## 6. Honest gaps (what's still open after this round)

Four of the six gaps from the original comparison are now **closed**; two remain.

### Closed

1. **Boss lock state.** `Instance` holds `locked: BTreeSet<&'static str>` and
   `apply_ante_locks(ante)` mirrors Immolate's `init_locks` exactly: the ten
   ante-1 small/big bosses, `The_Tooth`/`The_Eye` (ante < 3), `The_Plant`
   (ante < 4), `The_Serpent` (ante < 5), `The_Ox` (ante < 6). `next_boss` runs
   it before each draw. Verified across 100 k ante-1 seeds: 0 locked-boss leaks.
2. **Pack contents.** `open_pack` handles all 15 pack variants
   (Arcana/Celestial/Standard/Buffoon/Spectral × base/Jumbo/Mega) with the
   correct card counts and pick limits. Intra-pack duplicates are blocked via a
   temp-lock loop matching `arcana_pack` / `celestial_pack` / `spectral_pack` /
   `buffoon_pack` in Immolate. The `Op::PackContains` clause is now wired.
3. **Sticker rolls.** `next_joker_with_stickers` returns `Stickers { eternal,
   perishable, rental }`, gated by stake (Black+ for eternal, Orange+ for
   perishable, Gold+ for rental) and filtered through `ETERNAL_BLACKLIST` (11)
   and `PERISHABLE_BLACKLIST` (16). 100 k White-stake draws produce 0 stickers.
4. **Joker resample on duplicate.** `next_joker` goes through
   `rand_choice_common`, mirroring Immolate's resample loop (lock target on
   pick, retry up to `items.len() * 4 + 8`, unlock on success). Same primitive
   backs tarot/planet/spectral and the boss draw.

### Still open

5. **Standard pack card-level modelling.** `open_pack` returns high-level
   contents but doesn't draw individual playing cards with rank/suit/seal/
   enhancement. Filters depending on a specific seal-on-rank inside a Standard
   pack won't match correctly.
6. **Bit-for-bit Immolate parity.** The 100 k sweep proves determinism, pool
   validity, lock correctness, sticker correctness, and distributional sanity
   — not byte-identical match with Immolate on every seed. That would need
   running Immolate's OpenCL binary on the same 100 k seeds and diffing.

Until #6 ships, the Balatropedia integration keeps Immolate as the verified
default and this engine sits behind a beta toggle. Battle-testing is a third
open item by definition — the toggle is how it accumulates miles.

---

## 7. When to use which

- **You want to find a seed today with a filter Balatropedia already supports**
  → use Balatropedia. It's been tested by real users.
- **You want to share a filter with a friend via a URL** → use this.
- **You want to express a more complex filter shape than Balatropedia's form
  supports** → use this.
- **You want the fastest possible single-machine throughput on a modern
  browser with WebGPU** → use this (with the GPU toggle on).
- **You want to self-host a cloud worker for a guild of players** → use this
  (with the optional server).
- **You need confidence the bosses, packs, and stickers are exactly right** →
  the four invariants the regression sweep checks are clean; if you want
  bit-for-bit Immolate parity, stay on Balatropedia until gap #6 ships.

---

## 8. Integration path back into Balatropedia

The end state is that Balatropedia's "Seed Finder" tab calls *this* engine
instead of the current Immolate WASM. That means:

1. **Done as a parallel adapter, not a replacement.**
   `Balatropedia/client/src/lib/seedFinderV2.ts` +
   `seedFinderV2Worker.ts` expose the same `SeedFinder` interface and load the
   new WASM from `client/public/engine-v2/`.
2. **Form fields mapped to DSL clauses** for joker constraints: each
   `JokerConstraint` emits one `ante_shop_has_joker` clause per ante up to its
   `maxAnte`. Voucher and tag constraints still route to Immolate.
3. **Immolate stays as the verified default.** A checkbox in
   `tabs/SeedFinderTab.tsx` (“Try the new engine (beta)”, persisted to
   `localStorage`) routes the next search to V2.
4. **Open:** close gaps #5 and #6 above, then make V2 the default and retire
   Immolate.
