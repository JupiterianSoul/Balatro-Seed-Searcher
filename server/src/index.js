/**
 * Balatro Seed Searcher — Optional Cloud-Boost WebSocket Server
 *
 * This server is OFF BY DEFAULT. The web UI works fully offline without it.
 * It exists as an opt-in adapter for users who want to run searches on a
 * more powerful machine or cloud instance.
 *
 * Usage: cd server && npm install && npm start
 */

import { WebSocketServer } from 'ws';
import { Worker } from 'worker_threads';
import { existsSync } from 'fs';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';

const __dirname = dirname(fileURLToPath(import.meta.url));

const PORT = parseInt(process.env.PORT ?? '8787', 10);
const MAX_WORKERS = parseInt(process.env.MAX_WORKERS ?? '4', 10);

const ENGINE_PATH = join(__dirname, '../../engine/target/release/balatro-seed-engine');
const WORKER_PATH = join(__dirname, 'worker.js');

const wss = new WebSocketServer({ port: PORT });

console.log(`[server] Balatro Seed Searcher cloud-boost server listening on ws://localhost:${PORT}`);
console.log(`[server] MAX_WORKERS=${MAX_WORKERS}`);
console.log(`[server] Native engine path: ${ENGINE_PATH}`);
console.log(`[server] Engine present: ${existsSync(ENGINE_PATH)}`);

/** Track active workers per connection so we can kill them on disconnect. */
const connectionWorkers = new WeakMap();

wss.on('connection', (ws, req) => {
  const remote = req.socket.remoteAddress ?? 'unknown';
  console.log(`[server] Client connected: ${remote}`);

  const activeWorkers = new Set();
  connectionWorkers.set(ws, activeWorkers);

  ws.on('message', (raw) => {
    let msg;
    try {
      msg = JSON.parse(raw.toString());
    } catch {
      ws.send(JSON.stringify({ type: 'error', msg: 'invalid JSON' }));
      return;
    }

    if (msg.type !== 'scan') {
      ws.send(JSON.stringify({ type: 'error', msg: `unknown message type: ${msg.type}` }));
      return;
    }

    handleScan(ws, msg, activeWorkers);
  });

  ws.on('close', () => {
    console.log(`[server] Client disconnected: ${remote}`);
    // Kill all workers spawned for this connection
    for (const worker of activeWorkers) {
      worker.terminate();
    }
    activeWorkers.clear();
  });

  ws.on('error', (err) => {
    console.error(`[server] WebSocket error (${remote}):`, err.message);
  });
});

/**
 * Handle a 'scan' message. Spawns a worker_threads Worker to do the heavy
 * lifting so the event loop stays free for other connections.
 *
 * Expected message shape:
 *   { type: 'scan', startRank, count, filterJson, seedLen,
 *     deckIdx, stakeIdx, partial, minScore }
 */
function handleScan(ws, msg, activeWorkers) {
  const {
    startRank = 0,
    count = 1000,
    filterJson = '{}',
    seedLen = 8,
    deckIdx = 0,
    stakeIdx = 0,
    partial = false,
    minScore = 0,
  } = msg;

  // Guard: native engine must be built before real work can happen.
  if (!existsSync(ENGINE_PATH)) {
    ws.send(JSON.stringify({
      type: 'error',
      msg: 'native engine not built — run `cargo build --release` inside the engine/ crate first',
    }));
    // We still proceed with the stub worker so the protocol is exercisable
    // without the engine. Remove the early-return below once the engine crate
    // is wired up (milestone 3).
  }

  if (activeWorkers.size >= MAX_WORKERS) {
    ws.send(JSON.stringify({ type: 'error', msg: 'server busy — too many concurrent scans' }));
    return;
  }

  const workerData = {
    startRank,
    count,
    filterJson,
    seedLen,
    deckIdx,
    stakeIdx,
    partial,
    minScore,
    enginePath: ENGINE_PATH,
    engineExists: existsSync(ENGINE_PATH),
  };

  const worker = new Worker(WORKER_PATH, { workerData });
  activeWorkers.add(worker);

  // Heartbeat: forward progress pings every 100 ms so the client knows we're alive.
  let lastScanned = 0;
  const heartbeat = setInterval(() => {
    if (ws.readyState === ws.OPEN) {
      ws.send(JSON.stringify({ type: 'progress', scanned: lastScanned }));
    }
  }, 100);

  worker.on('message', (workerMsg) => {
    if (workerMsg.type === 'match') {
      lastScanned = workerMsg.rank ?? lastScanned;
      if (ws.readyState === ws.OPEN) {
        ws.send(JSON.stringify(workerMsg));
      }
    } else if (workerMsg.type === 'progress') {
      lastScanned = workerMsg.scanned ?? lastScanned;
    } else if (workerMsg.type === 'done') {
      clearInterval(heartbeat);
      if (ws.readyState === ws.OPEN) {
        ws.send(JSON.stringify({ type: 'done', scanned: workerMsg.scanned ?? count }));
      }
    }
  });

  worker.on('error', (err) => {
    clearInterval(heartbeat);
    console.error('[server] Worker error:', err.message);
    if (ws.readyState === ws.OPEN) {
      ws.send(JSON.stringify({ type: 'error', msg: `worker error: ${err.message}` }));
    }
    activeWorkers.delete(worker);
  });

  worker.on('exit', (code) => {
    clearInterval(heartbeat);
    if (code !== 0) {
      console.warn(`[server] Worker exited with code ${code}`);
    }
    activeWorkers.delete(worker);
  });
}
