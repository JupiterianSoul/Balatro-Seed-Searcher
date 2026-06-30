// Main-thread search orchestrator.
//
// Two execution modes:
//
//   1. THREADED  — page is `crossOriginIsolated` and `SharedArrayBuffer`
//                  exists. We spawn ONE `searchWorkerThreaded` that loads
//                  the threads engine bundle and runs rayon inside the
//                  WASM heap. The pool size is `hardwareConcurrency`.
//                  This is the preferred mode on the deployed pplx.app site
//                  (which sends COOP/COEP) and inside the Capacitor APK
//                  once we wire the same headers in the WebViewClient.
//
//   2. FALLBACK  — anything else (no SAB, no COOP/COEP, file:// pages,
//                  Capacitor without the right WebView config). We keep
//                  the existing N-worker model where each worker holds a
//                  separate WASM heap and partitions the seed range.
//
// The mode is decided once per `start()` call. Output messages from both
// paths follow the same protocol so the rest of this class doesn't care.

import type {
  Filter,
  SearchConfig,
  MatchRecord,
  WorkerInboundScan,
  WorkerInboundStop,
  WorkerOutbound,
  OrchestratorProgressEvent,
  OrchestratorMatchEvent,
  OrchestratorDoneEvent,
} from '../types';

// Total seed space: ranks 0 to 2^52 - 1 (Balatro uses 8-char base-36 seeds ≈ 2^41.4
// but the engine accepts a continuous rank range; we default to 2 billion for
// practical searching)
export const DEFAULT_SEARCH_SPACE = 2_000_000_000n; // 2 billion as a practical default

// 8-char seeds in base-35 — the engine's full enumeration domain.
// Used to wrap the per-run random starting offset so we never index outside
// representable seeds.
const FULL_SEED_DOMAIN_8 = 35n ** 8n; // ≈ 2.25e12

/**
 * Pick a random starting rank for a fresh search.
 *
 * Without this, every run starts at rank 0 (= seed "11111111") and walks
 * forward 2 billion ranks. Because 35^5 ≈ 52 million, the entire 2-billion
 * window stays inside seeds whose first three characters are "111". The user
 * sees "every result starts with the same letters".
 *
 * The randomized offset spreads the searched window across the full 8-char
 * seed space, so consecutive runs hit different prefixes and the results
 * look like the diverse set you get from a real seed finder.
 *
 * We keep the search CONTIGUOUS from the offset (rather than sampling
 * random seeds individually) so worker partitioning, dedup-by-rank, and
 * the rank-ordered top-N stay correct. The contiguity is invisible to the
 * user because the offset varies between runs.
 */
function randomStartRank(searchSpace: bigint): bigint {
  // crypto.getRandomValues gives us a uniform 64-bit value we then fold into
  // the available range. We subtract `searchSpace` so the run never tries to
  // index past the end of the 8-char domain — which would wrap inside the
  // engine and silently report duplicate seeds.
  const usable = FULL_SEED_DOMAIN_8 > searchSpace
    ? FULL_SEED_DOMAIN_8 - searchSpace
    : 1n;
  const buf = new BigUint64Array(1);
  crypto.getRandomValues(buf);
  return buf[0] % usable;
}

export type OrchestratorEventMap = {
  match: OrchestratorMatchEvent;
  progress: OrchestratorProgressEvent;
  done: OrchestratorDoneEvent;
};

type Listener<T> = (event: T) => void;

export class SearchOrchestrator {
  private workers: Worker[] = [];
  private listeners: {
    match: Listener<OrchestratorMatchEvent>[];
    progress: Listener<OrchestratorProgressEvent>[];
    done: Listener<OrchestratorDoneEvent>[];
  } = { match: [], progress: [], done: [] };

  // Aggregated state
  private matches: Map<bigint, MatchRecord> = new Map();
  private topN: MatchRecord[] = [];
  private topNLimit = 50;

  private workerScanned: Map<number, bigint> = new Map();
  private workerElapsed: Map<number, number> = new Map();
  private workerDone: Set<number> = new Set();
  private startTime = 0;
  private searchSpace = 0n;
  private running = false;
  private paused = false;
  private workerCount = 0;
  private threadedMode = false;

  private progressInterval: ReturnType<typeof setInterval> | null = null;

  // ─── Public API ────────────────────────────────────────────────────────────

  start(filter: Filter, config: SearchConfig, totalSpace = DEFAULT_SEARCH_SPACE): void {
    this.stop(); // Clean up any previous run

    this.matches.clear();
    this.topN = [];
    this.workerScanned.clear();
    this.workerElapsed.clear();
    this.workerDone.clear();
    this.startTime = performance.now();
    this.searchSpace = totalSpace;
    this.topNLimit = config.topN;
    this.running = true;
    this.paused = false;

    // Decide execution mode. Threaded needs COOP/COEP cross-origin isolation
    // plus SharedArrayBuffer. Both must be true; SAB without isolation is
    // gated by browsers and rayon will fail to spawn workers.
    const canThread =
      typeof crossOriginIsolated !== 'undefined' && crossOriginIsolated === true
      && typeof SharedArrayBuffer !== 'undefined';
    this.threadedMode = canThread;

    // Optional: when running inside the Android APK (MainActivity adds
    // AndroidDebug as a JS interface), echo the diagnostic to logcat so
    // we can confirm on-device that COOP/COEP landed and threading
    // actually engaged. No-ops on the open web.
    try {
      const ad: any = (globalThis as any).AndroidDebug;
      if (ad && typeof ad.log === 'function') {
        ad.log('crossOriginIsolated', String((globalThis as any).crossOriginIsolated));
        ad.log('SharedArrayBuffer', String(typeof SharedArrayBuffer !== 'undefined'));
        ad.log('hardwareConcurrency', String((navigator as any).hardwareConcurrency ?? 'n/a'));
        ad.log('engineMode', canThread ? 'threaded' : 'nworker-fallback');
      }
    } catch { /* no-op */ }

    // Random starting rank so successive searches don't all hammer the
    // same low-rank prefix. See randomStartRank() above for the rationale.
    const startOffset = randomStartRank(this.searchSpace);

    if (canThread) {
      // THREADED MODE — single worker, single WASM heap, rayon inside.
      this.workerCount = 1;
      const worker = new Worker(new URL('../workers/searchWorkerThreaded.ts', import.meta.url), {
        type: 'module',
      });
      worker.addEventListener('message', (event: MessageEvent<WorkerOutbound>) => {
        this.handleWorkerMessage(event.data);
      });
      const scanMsg: WorkerInboundScan = {
        type: 'scan',
        filter,
        startRank: startOffset,
        count: this.searchSpace,
        seedLen: config.seedLen,
        deckIdx: config.deckIdx,
        stakeIdx: config.stakeIdx,
        partial: filter.partial,
        minScore: filter.min_score ?? 0,
        workerId: 0,
      };
      worker.postMessage(scanMsg);
      this.workers.push(worker);
      this.workerScanned.set(0, 0n);
    } else {
      // FALLBACK MODE — N separate workers, each with its own WASM heap.
      const concurrency = Math.min(navigator.hardwareConcurrency || 4, 8);
      this.workerCount = concurrency;

      const sliceSize = this.searchSpace / BigInt(concurrency);

      for (let i = 0; i < concurrency; i++) {
        const worker = new Worker(new URL('../workers/searchWorker.ts', import.meta.url), {
          type: 'module',
        });

        const sliceLocal = BigInt(i) * sliceSize;
        const startRank = startOffset + sliceLocal;
        const count = i === concurrency - 1 ? this.searchSpace - sliceLocal : sliceSize;

        worker.addEventListener('message', (event: MessageEvent<WorkerOutbound>) => {
          this.handleWorkerMessage(event.data);
        });

        const scanMsg: WorkerInboundScan = {
          type: 'scan',
          filter,
          startRank,
          count,
          seedLen: config.seedLen,
          deckIdx: config.deckIdx,
          stakeIdx: config.stakeIdx,
          partial: filter.partial,
          minScore: filter.min_score ?? 0,
          workerId: i,
        };

        worker.postMessage(scanMsg);
        this.workers.push(worker);
        this.workerScanned.set(i, 0n);
      }
    }

    // Emit an immediate progress event so the UI starts ticking the moment
    // the user clicks Start, instead of waiting for the first worker message.
    this.emitProgress();

    // Emit progress every 100ms thereafter for smooth rate/elapsed updates
    this.progressInterval = setInterval(() => {
      if (this.running) this.emitProgress();
    }, 100);
  }

  stop(): void {
    if (this.progressInterval !== null) {
      clearInterval(this.progressInterval);
      this.progressInterval = null;
    }
    for (let i = 0; i < this.workers.length; i++) {
      const stopMsg: WorkerInboundStop = { type: 'stop', workerId: i };
      this.workers[i].postMessage(stopMsg);
      this.workers[i].terminate();
    }
    this.workers = [];
    this.running = false;
    this.paused = false;
  }

  pause(): void {
    // No native pause in workers; we stop them. Resume restarts from scratch.
    // For a real pause we'd need to track progress per worker and restart from there.
    this.paused = true;
    this.running = false;
    for (let i = 0; i < this.workers.length; i++) {
      const stopMsg: WorkerInboundStop = { type: 'stop', workerId: i };
      this.workers[i].postMessage(stopMsg);
    }
    if (this.progressInterval !== null) {
      clearInterval(this.progressInterval);
      this.progressInterval = null;
    }
  }

  resume(): void {
    this.paused = false;
  }

  isRunning(): boolean {
    return this.running;
  }

  isPaused(): boolean {
    return this.paused;
  }

  /**
   * Returns whether the current/last run used the threaded engine.
   * Useful for showing a small "multi-core" badge in the UI so the user
   * can tell whether their phone got the fast path.
   */
  isThreaded(): boolean {
    return this.threadedMode;
  }

  getTopMatches(): MatchRecord[] {
    return [...this.topN];
  }

  getTotalScanned(): bigint {
    let total = 0n;
    for (const v of this.workerScanned.values()) total += v;
    return total;
  }

  addEventListener<K extends keyof OrchestratorEventMap>(
    event: K,
    listener: Listener<OrchestratorEventMap[K]>,
  ): void {
    (this.listeners[event] as Listener<OrchestratorEventMap[K]>[]).push(listener);
  }

  removeEventListener<K extends keyof OrchestratorEventMap>(
    event: K,
    listener: Listener<OrchestratorEventMap[K]>,
  ): void {
    const arr = this.listeners[event] as Listener<OrchestratorEventMap[K]>[];
    const idx = arr.indexOf(listener);
    if (idx >= 0) arr.splice(idx, 1);
  }

  // ─── Internal ──────────────────────────────────────────────────────────────

  private handleWorkerMessage(msg: WorkerOutbound): void {
    if (msg.type === 'matches') {
      this.workerScanned.set(
        msg.workerId,
        (this.workerScanned.get(msg.workerId) ?? 0n) + msg.scanned,
      );
      this.ingestMatches(msg.matches);
    } else if (msg.type === 'progress') {
      this.workerScanned.set(msg.workerId, msg.scanned);
      this.workerElapsed.set(msg.workerId, msg.elapsedMs);
    } else if (msg.type === 'done') {
      this.workerScanned.set(
        msg.workerId,
        (this.workerScanned.get(msg.workerId) ?? 0n) + msg.totalScanned,
      );
      this.workerDone.add(msg.workerId);
      if (this.workerDone.size === this.workerCount) {
        this.onAllDone();
      }
    } else if (msg.type === 'error') {
      console.error(`Worker ${msg.workerId} error:`, msg.message);
    }
  }

  private ingestMatches(incoming: MatchRecord[]): void {
    let changed = false;
    for (const m of incoming) {
      if (!this.matches.has(m.rank)) {
        this.matches.set(m.rank, m);
        changed = true;
      }
    }
    if (!changed) return;

    // Rebuild sorted top-N: descending score, ascending rank
    this.topN = [...this.matches.values()]
      .sort((a, b) => {
        if (b.score !== a.score) return b.score - a.score;
        if (a.rank < b.rank) return -1;
        if (a.rank > b.rank) return 1;
        return 0;
      })
      .slice(0, this.topNLimit);

    const matchEvent: OrchestratorMatchEvent = { matches: incoming };
    for (const cb of this.listeners.match) cb(matchEvent);
  }

  private emitProgress(): void {
    const elapsedMs = performance.now() - this.startTime;
    const totalScanned = this.getTotalScanned();
    const seedsPerSec = elapsedMs > 0 ? Number(totalScanned) / (elapsedMs / 1000) : 0;

    const event: OrchestratorProgressEvent = {
      seedsPerSec,
      totalScanned,
      matchCount: this.matches.size,
      workerCount: this.workerCount,
      elapsedMs,
    };
    for (const cb of this.listeners.progress) cb(event);
  }

  private onAllDone(): void {
    if (this.progressInterval !== null) {
      clearInterval(this.progressInterval);
      this.progressInterval = null;
    }
    this.running = false;

    const elapsedMs = performance.now() - this.startTime;
    const doneEvent: OrchestratorDoneEvent = {
      totalScanned: this.getTotalScanned(),
      matchCount: this.matches.size,
      elapsedMs,
    };
    for (const cb of this.listeners.done) cb(doneEvent);
  }
}
