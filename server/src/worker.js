/**
 * Balatro Seed Searcher — Worker Thread Stub
 *
 * ─────────────────────────────────────────────────────────────────────────────
 * MILESTONE 3 — Real native engine wiring
 * ─────────────────────────────────────────────────────────────────────────────
 * This file is currently a PLACEHOLDER that emits fake match data every 50 ms
 * so the WebSocket protocol can be exercised end-to-end before the Rust engine
 * crate is compiled for the server's native target.
 *
 * To wire up the real engine:
 *   1. Build the engine crate:
 *        cd engine && cargo build --release
 *   2. Replace the stub loop below with a call to the native binary at
 *      `workerData.enginePath`, piping its stdout JSON lines back to the
 *      parent thread via `parentPort.postMessage(...)`.
 *   3. Parse the engine's output lines, which should match:
 *        { seed: string, rank: number, score: number }
 *      and forward them as `{ type: 'match', seed, rank, score }`.
 *   4. When the engine process exits, post `{ type: 'done', scanned: N }`.
 * ─────────────────────────────────────────────────────────────────────────────
 */

import { workerData, parentPort } from 'worker_threads';

const {
  startRank = 0,
  count = 1000,
  // enginePath and engineExists are available when the real engine is wired up:
  // enginePath,
  // engineExists,
} = workerData;

// ── Stub: emit placeholder matches ──────────────────────────────────────────

const STUB_INTERVAL_MS = 50;
const STUB_BATCH_SIZE = 10; // seeds "scanned" per tick

let scanned = 0;

/**
 * Generate a deterministic-looking fake seed string for a given rank.
 * Real seeds are uppercase alphanumeric, 8 characters long (for Balatro).
 */
function fakeSeed(rank) {
  const chars = 'ABCDEFGHJKLMNPQRSTUVWXYZ23456789';
  let s = '';
  let n = rank + 1;
  for (let i = 0; i < 8; i++) {
    s += chars[n % chars.length];
    n = Math.floor(n / chars.length) + 1;
  }
  return s;
}

const interval = setInterval(() => {
  const batchEnd = Math.min(scanned + STUB_BATCH_SIZE, count);

  for (let i = scanned; i < batchEnd; i++) {
    const rank = startRank + i;
    // Emit roughly 1-in-20 seeds as a "match" to simulate real hit rates.
    if (rank % 20 === 0) {
      parentPort.postMessage({
        type: 'match',
        seed: fakeSeed(rank),
        rank,
        score: Math.round(Math.random() * 100),
      });
    }
  }

  scanned = batchEnd;
  parentPort.postMessage({ type: 'progress', scanned: startRank + scanned });

  if (scanned >= count) {
    clearInterval(interval);
    parentPort.postMessage({ type: 'done', scanned: startRank + count });
  }
}, STUB_INTERVAL_MS);
