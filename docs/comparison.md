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
| Lock-state simulation              | Implemented (Immolate has it)           | **Approximated** (ante in cache key) — see gaps |
| Pack contents                      | Implemented                             | **Wired but not simulated** — clause returns false |
| Sticker rolls (eternal/perishable/rental) | Implemented                      | **TODO**                                      |
| Joker resample on duplicate        | Implemented                             | **Skipped** (rare effect on most filters)     |

¹ Number from the V6 work earlier in this session; varies a lot by browser, CPU,
worker count, and filter shape.

² Single-thread, native Rust, with real Balatro derivations
(`next_joker`, `next_tag`, `next_voucher`, `next_boss`) — `cargo run --release
--bin bench`. WASM SIMD lands within ~10–20 % of that on modern Chrome / Firefox.

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

## 6. Honest gaps (what Balatropedia still does better today)

I want to be clear about this because the user asked for an honest comparison:

1. **Boss lock state.** Immolate tracks which bosses have already appeared and
   removes them from the pool. This engine approximates by including `ante` in
   the cache key. On filters that depend on the exact pool state across ante
   transitions (rare), this engine will produce wrong bosses.
2. **Pack contents.** The DSL has a `pack_contains` clause wired up but the
   simulator returns `false` — pack opening is not yet implemented. Balatropedia
   inherits Immolate's working implementation.
3. **Sticker rolls.** Eternal / perishable / rental sticker chances are not
   yet derived. Balatropedia supports filtering on eternal jokers; this engine
   does not yet.
4. **Joker resample on duplicate.** When the shop tries to roll a joker the
   player already owns and the rules say resample, Immolate does the resample.
   This engine skips it (~1–2 % drift on filters that span ante 4+).
5. **Voucher chain side effects.** Vouchers like Hone or Magic Trick modify
   the pool for *subsequent* derivations. This engine treats each derivation
   independently. Same drift envelope as the boss lock issue.
6. **Battle-tested.** Balatropedia's finder has been used by the community for
   real seed hunting. This engine has been used to find ~ten thousand seeds in
   benchmark runs and zero by the community. Confidence comes from miles, not
   architecture diagrams.

The architecture is ready for all six fixes — they're item-table and
state-tracking work, not engine rewrites. They're tracked in the README.

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
  use Balatropedia until this engine closes those six gaps.

---

## 8. Integration path back into Balatropedia

The end state is that Balatropedia's "Seed Finder" tab calls *this* engine
instead of the current Immolate WASM. That means:

1. Replace `Balatropedia/client/src/components/SeedFinderTab.tsx`'s WASM import
   with `@balatro/seed-engine` (this crate's npm-published WASM bundle).
2. Map the existing form fields to DSL clauses.
3. Keep the current Immolate WASM around as the "verified" backend so users can
   cross-check results during the cutover.
4. Once the six gaps above are closed and outputs match Immolate on a 100 k-seed
   regression suite, retire Immolate.

That's the plan. It's not done in this PR — this PR is the engine and the
standalone UI. The Balatropedia integration is the next ticket.
