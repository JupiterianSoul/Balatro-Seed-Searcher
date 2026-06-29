// Threaded search worker (Phase 1 — wasm-bindgen-rayon).
//
// Loads `/engine-threads/balatro_seed_engine.js`, calls `initThreadPool` with
// `navigator.hardwareConcurrency`, then runs `scan_chunk_parallel` in batches
// over the whole assigned range. Rayon inside the WASM heap fans the work
// across nested workers using SharedArrayBuffer.
//
// This worker is only spawned when the orchestrator has already verified
// that the page is `crossOriginIsolated` AND `SharedArrayBuffer` exists.
// Otherwise the orchestrator falls back to the N-worker model in
// `searchWorker.ts`.
//
// Output protocol is identical to `searchWorker.ts` so the orchestrator's
// message handling does not need to know which mode is active.

import type {
  WorkerInbound,
  WorkerInboundScan,
  MatchRecord,
  WorkerOutbound,
} from '../types';

// ─── Threaded engine module shape ────────────────────────────────────────────

type ThreadedEngineModule = {
  default: (opts?: { module_or_path?: string }) => Promise<unknown>;
  init: () => void;
  initThreadPool: (num_threads: number) => Promise<unknown>;
  scan_chunk_parallel: (
    filter_json: string,
    start_rank: bigint,
    count: bigint,
    seed_len: number,
    deck_idx: number,
    stake_idx: number,
    partial: boolean,
    min_score: number,
  ) => Uint8Array;
};

// ─── Engine loading (with one-time cache) ────────────────────────────────────

let enginePromise: Promise<ThreadedEngineModule> | null = null;

function assetUrl(rel: string): string {
  const base = (import.meta.env.BASE_URL ?? '/').replace(/\/?$/, '/');
  return base + rel.replace(/^\/+/, '');
}

async function getEngine(): Promise<ThreadedEngineModule> {
  if (!enginePromise) {
    enginePromise = (async () => {
      const mod = (await import(/* @vite-ignore */ assetUrl('engine-threads/balatro_seed_engine.js'))) as ThreadedEngineModule;
      // WebAssembly.instantiateStreaming is used under the hood when the
      // browser supports it and the response has the right MIME type — the
      // wasm-pack generated loader will pick that path automatically. We
      // simply hand it the URL.
      await mod.default({ module_or_path: assetUrl('engine-threads/balatro_seed_engine_bg.wasm') });
      mod.init();

      const threads = Math.max(1, Math.min(navigator.hardwareConcurrency || 4, 16));
      await mod.initThreadPool(threads);
      return mod;
    })();
  }
  return enginePromise;
}

// ─── Record decoding (same wire format as serial path) ───────────────────────

const RECORD_SIZE = 17; // 8 (rank LE u64) + 1 (score) + 8 (seed ASCII padded)
const textDecoder = new TextDecoder('utf-8');

function decodeRecords(buf: Uint8Array): MatchRecord[] {
  const records: MatchRecord[] = [];
  const dv = new DataView(buf.buffer, buf.byteOffset, buf.byteLength);
  for (let offset = 0; offset + RECORD_SIZE <= buf.byteLength; offset += RECORD_SIZE) {
    const lo = dv.getUint32(offset, true);
    const hi = dv.getUint32(offset + 4, true);
    const rank = BigInt(lo) | (BigInt(hi) << 32n);
    const score = dv.getUint8(offset + 8);
    const seedBytes = buf.slice(offset + 9, offset + 17);
    const seed = textDecoder.decode(seedBytes).trimEnd();
    records.push({ rank, score, seed });
  }
  return records;
}

// ─── Per-worker stop flag ────────────────────────────────────────────────────

const stopFlags = new Set<number>();

// ─── Adaptive batching ───────────────────────────────────────────────────────
//
// We still batch the seed range (1M-default first batch) so the UI keeps
// getting progress events and the user can cancel. Rayon parallelises
// inside each batch — so a single batch ALREADY uses all cores.

const TARGET_BATCH_MS = 400;
const MIN_BATCH = 50_000n;
const MAX_BATCH = 8_000_000n;
const PROGRESS_INTERVAL_MS = 100;
const INITIAL_BATCH = 200_000n;

async function handleScan(msg: WorkerInboundScan): Promise<void> {
  const { workerId, filter, startRank, seedLen, deckIdx, stakeIdx, partial, minScore } = msg;
  let remaining = msg.count;
  let currentRank = startRank;
  let batchSize = msg.count < INITIAL_BATCH ? msg.count : INITIAL_BATCH;
  let totalScanned = 0n;
  const searchStart = performance.now();
  let lastProgressMs = searchStart - PROGRESS_INTERVAL_MS;

  stopFlags.delete(workerId);

  // Liveness ping so the UI starts ticking before WASM finishes loading.
  const aliveMsg: WorkerOutbound = {
    type: 'progress',
    workerId,
    scanned: 0n,
    elapsedMs: 0,
  };
  self.postMessage(aliveMsg);

  let engine: ThreadedEngineModule;
  try {
    engine = await getEngine();
  } catch (e) {
    const errMsg: WorkerOutbound = {
      type: 'error',
      workerId,
      message: `threaded engine init failed: ${String(e)}`,
    };
    self.postMessage(errMsg);
    return;
  }

  const filterJson = JSON.stringify(filter);

  while (remaining > 0n && !stopFlags.has(workerId)) {
    const thisBatch = remaining < batchSize ? remaining : batchSize;
    const batchStart = performance.now();

    let raw: Uint8Array;
    try {
      raw = engine.scan_chunk_parallel(
        filterJson,
        currentRank,
        thisBatch,
        seedLen,
        deckIdx,
        stakeIdx,
        partial,
        minScore,
      );
    } catch (e) {
      const errMsg: WorkerOutbound = {
        type: 'error',
        workerId,
        message: `scan_chunk_parallel failed: ${String(e)}`,
      };
      self.postMessage(errMsg);
      return;
    }

    const batchMs = performance.now() - batchStart;

    const matches = decodeRecords(raw);
    if (matches.length > 0) {
      const matchMsg: WorkerOutbound = {
        type: 'matches',
        workerId,
        matches,
        scanned: thisBatch,
      };
      self.postMessage(matchMsg);
    }

    currentRank += thisBatch;
    remaining -= thisBatch;
    totalScanned += thisBatch;

    // Adapt batch size — target TARGET_BATCH_MS. Rayon is using all cores
    // so big batches are fine; small batches are paying dispatch overhead.
    if (batchMs > 0) {
      const ratio = TARGET_BATCH_MS / batchMs;
      let next = BigInt(Math.round(Number(batchSize) * ratio));
      if (next < MIN_BATCH) next = MIN_BATCH;
      if (next > MAX_BATCH) next = MAX_BATCH;
      batchSize = next;
    }

    const now = performance.now();
    if (now - lastProgressMs >= PROGRESS_INTERVAL_MS) {
      const progressMsg: WorkerOutbound = {
        type: 'progress',
        workerId,
        scanned: totalScanned,
        elapsedMs: now - searchStart,
      };
      self.postMessage(progressMsg);
      lastProgressMs = now;
    }
  }

  const doneMsg: WorkerOutbound = {
    type: 'done',
    workerId,
    totalScanned,
  };
  self.postMessage(doneMsg);
}

self.addEventListener('message', (event: MessageEvent<WorkerInbound>) => {
  const msg = event.data;
  if (msg.type === 'scan') {
    void handleScan(msg);
  } else if (msg.type === 'stop') {
    stopFlags.add(msg.workerId);
  }
});
