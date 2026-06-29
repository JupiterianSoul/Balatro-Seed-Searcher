#!/usr/bin/env node
// Statistical parity harness: Immolate (ground truth) vs Engine V2 (Rust+WASM).
//
// Methodology
// -----------
// 1. For each representative clause shape, run BOTH engines from disjoint
//    starting points for the same fixed wall-clock budget.
// 2. Compute the empirical hit rate per engine.
// 3. Report the absolute and relative deltas. A bit-for-bit parity would
//    show both engines finding the SAME seeds; a statistical parity (this
//    harness) only confirms they agree on the underlying probability.
//
// This is the "honest gap" closer for v2.1: we can't do a 1M-seed
// side-by-side comparison in a sandbox, but we CAN cross-check that the
// observed hit densities match within statistical noise.
//
// Run from repo root:
//   node scripts/parity_harness.js
//
// Outputs a markdown table to stdout and writes scripts/parity_results.json.

const fs = require("node:fs");
const path = require("node:path");

const REPO = path.resolve(__dirname, "..");
const IMM_DIR = path.resolve(REPO, "../Balatropedia/client/public/wasm");
const ENGINE_PKG = path.resolve(REPO, "engine/pkg-node");

// Wall-clock budget per engine per clause. 10s is enough for >1M tries on
// either engine on the 2 vCPU sandbox.
const PER_CASE_BUDGET_MS = 15_000;

// ─── Test cases ──────────────────────────────────────────────────────────────
// Each case carries (a) the Engine-V2 JSON filter and (b) the equivalent
// Immolate constraint vectors. We restrict ourselves to clauses Immolate
// natively supports.
// IMPORTANT methodological note:
//   Immolate's `findSeedV2` with source="" matches the joker ANYWHERE
//   (shop slot 0..3, pack contents, Soul drops, etc.). Our engine's
//   ante_shop_has_joker { slot: 0 } only matches the FIRST shop slot.
//   To compare apples-to-apples we need both sides to query the same
//   space. We use source="shop" on Immolate (still scans default 4 slots,
//   not 16) and slot=255 (any of 0..15) on the engine.
const CASES = [
  {
    name: "Blueprint in shop, ante 1..8",
    immolate: {
      jokerConstraints: [{ joker: "Blueprint", edition: "", source: "shop", maxAnte: 8 }],
      voucherConstraints: [],
      tagConstraints: [],
    },
    engine: {
      clauses: [{
        kind: "any_of", clauses: Array.from({ length: 8 }, (_, i) => (
          { kind: "ante_shop_has_joker", ante: i + 1, slot: 255, joker: "Blueprint" }
        )),
      }],
      partial: false,
      min_score: null,
    },
    // Honest gap: engine scans 16 slots (0..15) while Immolate scans 4.
    // We expect V2 rate ≈ 4× Immolate rate for any rare joker. Comparing
    // shapes, not absolute numbers — we look for the rates to be of the
    // same order of magnitude and stable across runs.
    expectedRate: null,
  },
  {
    name: "Negative Tag (any blind) ante 1",
    immolate: {
      jokerConstraints: [],
      voucherConstraints: [],
      tagConstraints: [{ tag: "Negative Tag", maxAnte: 1 }],
    },
    engine: {
      clauses: [
        { kind: "any_of", clauses: [
          { kind: "ante_tag_is", ante: 1, position: 0, tag: "Negative Tag" },
          { kind: "ante_tag_is", ante: 1, position: 1, tag: "Negative Tag" },
        ] },
      ],
      partial: false,
      min_score: null,
    },
    expectedRate: 0.085, // 24 tags, 2 positions => 1 - (23/24)^2 ≈ 0.082
  },
  {
    // Engine + Immolate both walk the same shop-rate chain for jokers in
    // the default 4 slots; this is the strongest joker-side parity check.
    name: "Common Greedy Joker in shop, ante 1",
    immolate: {
      jokerConstraints: [{ joker: "Greedy Joker", edition: "", source: "shop", maxAnte: 1 }],
      voucherConstraints: [],
      tagConstraints: [],
    },
    engine: {
      // Slot 0 only (matches Immolate's default 4-slot scan more closely
      // than 16-slot any-of would; Immolate scans 0..3, engine 0..15 with
      // sentinel, so we keep them at slot 0 for the cleanest comparison).
      clauses: [
        { kind: "ante_shop_has_joker", ante: 1, slot: 0, joker: "Greedy Joker" },
      ],
      partial: false,
      min_score: null,
    },
    expectedRate: null, // engine narrower than Immolate — expect engine < Imm
  },
];

// ─── Engine V2 runner ────────────────────────────────────────────────────────
let _engineCache = null;
function loadEngine() {
  if (_engineCache) return _engineCache;
  // The nodejs-target wasm-pack output exports init and scan_chunk directly;
  // it lazy-loads the .wasm file beside the .js when the first function is
  // called.
  _engineCache = require(path.join(ENGINE_PKG, "balatro_seed_engine.js"));
  _engineCache.init();
  return _engineCache;
}

async function runEngineCase(c) {
  const eng = loadEngine();

  const filter = JSON.stringify(c.engine);
  const SEED_LEN = 8;
  const BATCH = 200_000;

  // Use a random starting rank so consecutive runs don't repeat.
  let cursor = BigInt(Math.floor(Math.random() * 1e10));
  let totalScanned = 0n;
  let matches = 0;
  const t0 = Date.now();

  while (Date.now() - t0 < PER_CASE_BUDGET_MS) {
    const raw = eng.scan_chunk(filter, cursor, BigInt(BATCH), SEED_LEN, 0, 0, false, 0);
    // Each record = 8 (rank LE) + 1 (score) + 8 (seed); 17 bytes total.
    matches += Math.floor(raw.length / 17);
    cursor += BigInt(BATCH);
    totalScanned += BigInt(BATCH);
  }
  return {
    scanned: Number(totalScanned),
    matches,
    rate: matches / Number(totalScanned),
  };
}

// ─── Immolate runner ─────────────────────────────────────────────────────────
let _immolateModule = null;
async function getImmolate() {
  if (_immolateModule) return _immolateModule;
  // The Immolate WASM bundle is built for browsers but it does run under
  // Node via Module._compile. We bypass `require` here because some Node
  // versions cache the file under a different key and hand back an empty
  // exports object on the second load.
  const NodeModule = require("module");
  const immJsPath = path.join(IMM_DIR, "immolate.js");
  const code = fs.readFileSync(immJsPath, "utf8");
  const m = new NodeModule(immJsPath);
  m.filename = immJsPath;
  m.paths = NodeModule._nodeModulePaths(IMM_DIR);
  m._compile(code, immJsPath);
  const Immolate = m.exports;
  if (typeof Immolate !== "function") {
    throw new Error("Immolate did not load as a function (got " + typeof Immolate + ")");
  }
  _immolateModule = await Immolate({
    locateFile: (p) => p.endsWith(".wasm") ? path.join(IMM_DIR, "immolate.wasm") : p,
    print: () => {}, printErr: () => {},
  });
  return _immolateModule;
}

async function runImmolateCase(c) {
  const Module = await getImmolate();

  const jc = new Module.VectorJokerConstraint();
  for (const x of c.immolate.jokerConstraints) jc.push_back(x);
  const vc = new Module.VectorVoucherConstraint();
  for (const x of c.immolate.voucherConstraints) vc.push_back(x);
  const tc = new Module.VectorTagConstraint();
  for (const x of c.immolate.tagConstraints) tc.push_back(x);

  // Immolate's findSeedV2 returns at most ONE match per call (the first hit).
  // `tries` is the number of seeds checked before finding it (or, if no
  // match found, the budget we requested). We accumulate the total seeds
  // checked and the total matches found.
  //
  // When tries < BATCH it means a hit was found at position `tries` in the
  // batch — we count exactly 1 match and `tries` seeds scanned. When tries
  // == BATCH (no hit), we count 0 matches and BATCH seeds scanned.
  let totalScanned = 0;
  let matches = 0;
  let rngSeed = (Math.random() * 0xffffffff) >>> 0;
  const BATCH = 50_000;
  const maxAnte = Math.max(
    1,
    ...c.immolate.jokerConstraints.map(x => x.maxAnte),
    ...c.immolate.voucherConstraints.map(x => x.maxAnte),
    ...c.immolate.tagConstraints.map(x => x.maxAnte),
  );
  const t0 = Date.now();
  while (Date.now() - t0 < PER_CASE_BUDGET_MS) {
    const res = Module.findSeedV2(
      rngSeed, BATCH, maxAnte, "Red Deck", "White Stake", 10106, jc, vc, tc,
    );
    if (res.seed) {
      matches += 1;
      totalScanned += res.tries;
    } else {
      totalScanned += BATCH;
    }
    // Bump cursor past the consumed range.
    rngSeed = (rngSeed + (res.tries || BATCH) + 1) >>> 0;
  }

  jc.delete(); vc.delete(); tc.delete();
  return {
    scanned: totalScanned,
    matches,
    rate: matches / totalScanned,
  };
}

// ─── Main ────────────────────────────────────────────────────────────────────
(async () => {
  const results = [];
  for (const c of CASES) {
    process.stdout.write(`Running '${c.name}' ... `);
    const tEng = Date.now();
    const eng = await runEngineCase(c).catch(e => ({ error: String(e) }));
    const engMs = Date.now() - tEng;
    const tImm = Date.now();
    const imm = await runImmolateCase(c).catch(e => ({ error: String(e) }));
    const immMs = Date.now() - tImm;
    process.stdout.write("done\n");
    results.push({ name: c.name, expectedRate: c.expectedRate, eng, engMs, imm, immMs });
  }

  // Markdown table
  console.log("\n## Parity Results\n");
  console.log("| Case | Engine V2 rate | Immolate rate | Expected | Δ (V2−Imm) | Within noise? |");
  console.log("|---|---|---|---|---|---|");
  for (const r of results) {
    const er = r.eng.error ? "ERR" : r.eng.rate.toExponential(2);
    const ir = r.imm.error ? "ERR" : r.imm.rate.toExponential(2);
    let delta = "n/a", noise = "n/a";
    if (!r.eng.error && !r.imm.error) {
      const d = r.eng.rate - r.imm.rate;
      delta = (d >= 0 ? "+" : "") + d.toExponential(2);
      // Pooled sample-proportion 95% CI half-width
      const n = Math.min(r.eng.scanned, r.imm.scanned);
      const p = (r.eng.matches + r.imm.matches) / (r.eng.scanned + r.imm.scanned);
      const halfWidth = 1.96 * Math.sqrt(2 * p * (1 - p) / n);
      noise = Math.abs(d) < halfWidth ? "yes" : "NO";
    }
    const exp = r.expectedRate == null ? "—" : r.expectedRate.toExponential(2);
    console.log(`| ${r.name} | ${er} | ${ir} | ${exp} | ${delta} | ${noise} |`);
  }

  fs.writeFileSync(path.join(__dirname, "parity_results.json"), JSON.stringify(results, null, 2));
  console.log("\nResults written to scripts/parity_results.json");
})();
