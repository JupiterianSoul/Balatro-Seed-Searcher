// Main-thread search orchestrator.
// Spawns N workers (up to 8), partitions seed space, aggregates results.

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
// but the engine accepts a continuous rank range; we default to 2^32 for practical searching)
export const DEFAULT_SEARCH_SPACE = 2_000_000_000n; // 2 billion as a practical default

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

    const concurrency = Math.min(navigator.hardwareConcurrency || 4, 8);
    this.workerCount = concurrency;

    // Partition seed space: worker i scans ranks i, i+n, i+2n, ...
    // Each worker gets a contiguous slice for simplicity (lower overhead).
    // Interleaved would be ideal but a big contiguous slice is fine for seed searching.
    const sliceSize = this.searchSpace / BigInt(concurrency);

    for (let i = 0; i < concurrency; i++) {
      const worker = new Worker(new URL('../workers/searchWorker.ts', import.meta.url), {
        type: 'module',
      });

      const startRank = BigInt(i) * sliceSize;
      const count = i === concurrency - 1 ? this.searchSpace - startRank : sliceSize;

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
