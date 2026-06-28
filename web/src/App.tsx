import React, { useCallback, useEffect, useRef, useState } from 'react';
import { FilterBuilder } from './components/FilterBuilder';
import { ResultsPanel } from './components/ResultsPanel';
import { SearchOrchestrator, DEFAULT_SEARCH_SPACE } from './search/orchestrator';
import type { Filter, SearchConfig, MatchRecord } from './types';
import { DECK_NAMES, STAKE_NAMES } from './types';
import './styles.css';

// ─── URL hash filter encoding ─────────────────────────────────────────────────

function tryDecodeFilterFromHash(): Filter | null {
  try {
    const hash = window.location.hash;
    const match = hash.match(/[#&]filter=([^&]*)/);
    if (!match) return null;
    const json = decodeURIComponent(escape(atob(match[1])));
    return JSON.parse(json) as Filter;
  } catch {
    return null;
  }
}

const DEFAULT_FILTER: Filter = {
  clauses: [],
  partial: false,
};

const DEFAULT_CONFIG: SearchConfig = {
  seedLen: 8,
  deckIdx: 0,
  stakeIdx: 0,
  topN: 50,
};

// ─── Engine status pill ───────────────────────────────────────────────────────

type EngineStatus = 'detecting' | 'simd' | 'scalar';

async function detectSimd(): Promise<boolean> {
  const bytes = new Uint8Array([
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00,
    0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7b,
    0x03, 0x02, 0x01, 0x00,
    0x0a, 0x0a, 0x01, 0x08, 0x00, 0x41, 0x00, 0xfd, 0x0f, 0xfd, 0x62, 0x0b,
  ]);
  try {
    return WebAssembly.validate(bytes);
  } catch {
    return false;
  }
}

// ─── App ──────────────────────────────────────────────────────────────────────

export default function App(): React.JSX.Element {
  const [filter, setFilter] = useState<Filter>(tryDecodeFilterFromHash() ?? DEFAULT_FILTER);
  const [config, setConfig] = useState<SearchConfig>(DEFAULT_CONFIG);

  const [isRunning, setIsRunning] = useState(false);
  const [isPaused, setIsPaused] = useState(false);
  const [isDone, setIsDone] = useState(false);

  const [matches, setMatches] = useState<MatchRecord[]>([]);
  const [seedsPerSec, setSeedsPerSec] = useState(0);
  const [totalScanned, setTotalScanned] = useState(0n);
  const [workerCount, setWorkerCount] = useState(0);

  const [engineStatus, setEngineStatus] = useState<EngineStatus>('detecting');

  const orchestratorRef = useRef<SearchOrchestrator | null>(null);

  // Detect SIMD on mount
  useEffect(() => {
    void detectSimd().then(simd => setEngineStatus(simd ? 'simd' : 'scalar'));
  }, []);

  // Lazy-create orchestrator
  function getOrchestrator(): SearchOrchestrator {
    if (!orchestratorRef.current) {
      orchestratorRef.current = new SearchOrchestrator();
    }
    return orchestratorRef.current;
  }

  const handleStart = useCallback(() => {
    setIsDone(false);
    setMatches([]);
    setSeedsPerSec(0);
    setTotalScanned(0n);

    const orc = getOrchestrator();

    // Remove old listeners by recreating the orchestrator
    orc.stop();
    orchestratorRef.current = new SearchOrchestrator();
    const fresh = orchestratorRef.current;

    fresh.addEventListener('match', () => {
      setMatches(fresh.getTopMatches());
    });

    fresh.addEventListener('progress', ev => {
      setSeedsPerSec(ev.seedsPerSec);
      setTotalScanned(ev.totalScanned);
      setWorkerCount(ev.workerCount);
    });

    fresh.addEventListener('done', ev => {
      setIsRunning(false);
      setIsDone(true);
      setTotalScanned(ev.totalScanned);
      setSeedsPerSec(0);
      setMatches(fresh.getTopMatches());
    });

    fresh.start(filter, config, DEFAULT_SEARCH_SPACE);
    setIsRunning(true);
    setIsPaused(false);
    setWorkerCount(Math.min(navigator.hardwareConcurrency || 4, 8));
  }, [filter, config]);

  const handleStop = useCallback(() => {
    orchestratorRef.current?.stop();
    setIsRunning(false);
    setIsPaused(false);
  }, []);

  const handlePause = useCallback(() => {
    orchestratorRef.current?.pause();
    setIsRunning(false);
    setIsPaused(true);
  }, []);

  // Persist filter to URL hash
  useEffect(() => {
    try {
      const json = JSON.stringify(filter);
      const encoded = btoa(unescape(encodeURIComponent(json)));
      history.replaceState(null, '', `#filter=${encoded}`);
    } catch {
      // ignore
    }
  }, [filter]);

  return (
    <div className="app-root">
      {/* Header */}
      <header className="app-header">
        <div className="header-inner">
          <div className="header-brand">
            {/* SVG Logo */}
            <svg
              className="logo-svg"
              width="36"
              height="36"
              viewBox="0 0 36 36"
              fill="none"
              aria-label="Balatro Seed Searcher logo"
              xmlns="http://www.w3.org/2000/svg"
            >
              <rect x="2" y="2" width="32" height="32" rx="8" fill="#7c3aed" />
              <text x="18" y="25" textAnchor="middle" fontSize="20" fill="white" fontWeight="bold" fontFamily="monospace">B</text>
              <circle cx="27" cy="9" r="5" fill="#a78bfa" />
              <line x1="24" y1="12" x2="20" y2="16" stroke="white" strokeWidth="1.5" strokeLinecap="round" />
            </svg>
            <h1 className="header-title">Balatro Seed Searcher</h1>
          </div>
          <div className="header-meta">
            {engineStatus === 'detecting' && (
              <span className="engine-pill engine-pill--detecting">Detecting engine…</span>
            )}
            {engineStatus === 'simd' && (
              <span className="engine-pill engine-pill--simd">WASM SIMD</span>
            )}
            {engineStatus === 'scalar' && (
              <span className="engine-pill engine-pill--scalar">WASM scalar</span>
            )}
          </div>
        </div>
      </header>

      {/* Main layout */}
      <main className="app-main">
        <div className="app-layout">
          {/* Left: Filter + config */}
          <aside className="app-sidebar">
            <FilterBuilder filter={filter} onChange={setFilter} />

            {/* Search config card */}
            <div className="card config-card">
              <h2 className="card-title">Search config</h2>

              <div className="config-row">
                <label className="config-label" htmlFor="deck-select">Deck</label>
                <select
                  id="deck-select"
                  className="input-select input-select--wide"
                  value={config.deckIdx}
                  onChange={e => setConfig({ ...config, deckIdx: Number(e.target.value) })}
                >
                  {DECK_NAMES.map((name, i) => (
                    <option key={i} value={i}>{name}</option>
                  ))}
                </select>
              </div>

              <div className="config-row">
                <label className="config-label" htmlFor="stake-select">Stake</label>
                <select
                  id="stake-select"
                  className="input-select input-select--wide"
                  value={config.stakeIdx}
                  onChange={e => setConfig({ ...config, stakeIdx: Number(e.target.value) })}
                >
                  {STAKE_NAMES.map((name, i) => (
                    <option key={i} value={i}>{name}</option>
                  ))}
                </select>
              </div>

              <div className="config-row">
                <label className="config-label" htmlFor="seedlen-select">Seed length</label>
                <select
                  id="seedlen-select"
                  className="input-select"
                  value={config.seedLen}
                  onChange={e => setConfig({ ...config, seedLen: Number(e.target.value) })}
                >
                  {[4, 5, 6, 7, 8].map(l => (
                    <option key={l} value={l}>{l} chars</option>
                  ))}
                </select>
              </div>

              <div className="config-row">
                <label className="config-label" htmlFor="topn-select">Top N results</label>
                <select
                  id="topn-select"
                  className="input-select"
                  value={config.topN}
                  onChange={e => setConfig({ ...config, topN: Number(e.target.value) })}
                >
                  {[10, 25, 50, 100, 200].map(n => (
                    <option key={n} value={n}>{n}</option>
                  ))}
                </select>
              </div>

              {!isRunning && (
                <button
                  className="btn btn--primary btn--start"
                  onClick={handleStart}
                  disabled={filter.clauses.length === 0}
                >
                  {isDone ? '↺ Search again' : '▶ Start search'}
                </button>
              )}
              {isRunning && (
                <div className="start-actions">
                  <button className="btn btn--secondary" onClick={handlePause}>⏸ Pause</button>
                  <button className="btn btn--danger-outline" onClick={handleStop}>■ Stop</button>
                </div>
              )}
              {isPaused && !isRunning && (
                <div className="start-actions">
                  <button className="btn btn--primary" onClick={handleStart}>▶ Resume</button>
                  <button className="btn btn--danger-outline" onClick={handleStop}>■ Stop</button>
                </div>
              )}
              {filter.clauses.length === 0 && !isRunning && (
                <p className="hint-text">Add at least one clause to enable search.</p>
              )}
            </div>
          </aside>

          {/* Right: Results */}
          <section className="app-content">
            <ResultsPanel
              matches={matches}
              seedsPerSec={seedsPerSec}
              workerCount={workerCount}
              totalScanned={totalScanned}
              isRunning={isRunning}
              isPaused={isPaused}
              isDone={isDone}
              filter={filter}
              onStart={handleStart}
              onStop={handleStop}
              onPause={handlePause}
            />
          </section>
        </div>
      </main>

      {/* Footer */}
      <footer className="app-footer">
        <div className="footer-inner">
          <a
            href="https://github.com/user/Balatro-Seed-Searcher"
            target="_blank"
            rel="noopener noreferrer"
            className="footer-link"
          >
            GitHub
          </a>
          <span className="footer-sep">·</span>
          <span className="footer-tagline">
            Built without a single line of copied Balatro source code
          </span>
        </div>
      </footer>
    </div>
  );
}
