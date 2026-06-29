# Changelog

All notable changes to the Balatro Seed Searcher engine.

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
