// Web Worker: loads WASM engine on first message, then scans seed chunks
// and streams results back to the main thread.

import type { EngineModule } from '../engine/loader';
import type {
  WorkerInbound,
  WorkerInboundScan,
  MatchRecord,
  WorkerOutbound,
} from '../types';

// ─── SIMD detection (duplicated here — workers have no shared module scope) ───

const SIMD_TEST_BYTES = new Uint8Array([
  0x00, 0x61, 0x73, 0x6d,
  0x01, 0x00, 0x00, 0x00,
  0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7b,
  0x03, 0x02, 0x01, 0x00,
  0x0a, 0x0a, 0x01, 0x08, 0x00, 0x41, 0x00, 0xfd, 0x0f, 0xfd, 0x62, 0x0b,
]);

function detectSimd(): boolean {
  try {
    return WebAssembly.validate(SIMD_TEST_BYTES);
  } catch {
    return false;
  }
}

// ─── Engine loading ──────────────────────────────────────────────────────────

let enginePromise: Promise<EngineModule> | null = null;

async function getEngine(): Promise<EngineModule> {
  if (!enginePromise) {
    enginePromise = (async () => {
      const simd = detectSimd();
      const basePath = simd ? '/engine-simd' : '/engine';
      const mod = await import(/* @vite-ignore */ `${basePath}/balatro_seed_engine.js`) as {
        default: (opts?: { module_or_path?: string }) => Promise<unknown>;
        init: () => void;
        scan_chunk: (
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
      await mod.default({ module_or_path: `${basePath}/balatro_seed_engine_bg.wasm` });
      mod.init();
      return { init: mod.init, scan_chunk: mod.scan_chunk };
    })();
  }
  return enginePromise;
}

// ─── Record decoding ─────────────────────────────────────────────────────────

const RECORD_SIZE = 17; // 8 (rank LE) + 1 (score) + 8 (seed)
const textDecoder = new TextDecoder('utf-8');

function decodeRecords(buf: Uint8Array): MatchRecord[] {
  const records: MatchRecord[] = [];
  const dv = new DataView(buf.buffer, buf.byteOffset, buf.byteLength);

  for (let offset = 0; offset + RECORD_SIZE <= buf.byteLength; offset += RECORD_SIZE) {
    // Rank: 8-byte little-endian u64
    const lo = dv.getUint32(offset, true);
    const hi = dv.getUint32(offset + 4, true);
    const rank = BigInt(lo) | (BigInt(hi) << 32n);

    // Score: 1 byte
    const score = dv.getUint8(offset + 8);

    // Seed: 8 bytes, right-padded with spaces — trim trailing spaces
    const seedBytes = buf.slice(offset + 9, offset + 17);
    const seed = textDecoder.decode(seedBytes).trimEnd();

    records.push({ rank, score, seed });
  }
  return records;
}

// ─── Stop flag per worker ID ─────────────────────────────────────────────────

const stopFlags = new Set<number>();

// ─── Message handler ─────────────────────────────────────────────────────────

// Adaptive batch sizing targets ~250ms per call
const TARGET_BATCH_MS = 250;
const MIN_BATCH = 1000n;
const MAX_BATCH = 500_000n;
const PROGRESS_INTERVAL_MS = 500;

async function handleScan(msg: WorkerInboundScan): Promise<void> {
  const { workerId, filter, startRank, seedLen, deckIdx, stakeIdx, partial, minScore } = msg;
  let remaining = msg.count;
  let currentRank = startRank;
  let batchSize = msg.count < 10_000n ? msg.count : 10_000n;
  let totalScanned = 0n;
  const searchStart = performance.now();
  let lastProgressMs = searchStart;

  stopFlags.delete(workerId);

  let engine: EngineModule;
  try {
    engine = await getEngine();
  } catch (e) {
    const errMsg: WorkerOutbound = {
      type: 'error',
      workerId,
      message: String(e),
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
      raw = engine.scan_chunk(
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
        message: String(e),
      };
      self.postMessage(errMsg);
      return;
    }

    const batchMs = performance.now() - batchStart;

    // Decode and emit matches if any
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

    // Adaptive batch: target 250ms per batch
    if (batchMs > 0) {
      const ratio = TARGET_BATCH_MS / batchMs;
      let next = BigInt(Math.round(Number(batchSize) * ratio));
      if (next < MIN_BATCH) next = MIN_BATCH;
      if (next > MAX_BATCH) next = MAX_BATCH;
      batchSize = next;
    }

    // Periodic progress report
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

  // Final done message
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
