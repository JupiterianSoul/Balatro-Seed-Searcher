import React, { useCallback, useEffect, useRef, useState } from 'react';
import { FilterBuilder } from './components/FilterBuilder';
import { ResultsPanel } from './components/ResultsPanel';
import { SearchOrchestrator, DEFAULT_SEARCH_SPACE } from './search/orchestrator';
import { assetPath } from './engine/assetPath';
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

// ─── Local storage persistence (phase 2 — APK cold-start polish) ─────────────
//
// We persist the last filter+config so that if a phone evicts the WebView
// and restores it, the user lands back on the same search instead of an
// empty form. URL hash already carries the filter, but on the APK there's
// no shareable URL — localStorage is the survivable copy.

const LS_FILTER_KEY = 'seed-searcher-last-filter-v1';
const LS_CONFIG_KEY = 'seed-searcher-last-config-v1';

function tryLoadFilterFromLocalStorage(): Filter | null {
  try {
    const raw = localStorage.getItem(LS_FILTER_KEY);
    if (!raw) return null;
    return JSON.parse(raw) as Filter;
  } catch {
    return null;
  }
}

function tryLoadConfigFromLocalStorage(): SearchConfig | null {
  try {
    const raw = localStorage.getItem(LS_CONFIG_KEY);
    if (!raw) return null;
    return JSON.parse(raw) as SearchConfig;
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
  // Filter precedence: URL hash (shareable link) wins over localStorage
  // (last session). The hash is empty on a fresh APK launch, so localStorage
  // is the natural fallback.
  const [filter, setFilter] = useState<Filter>(
    tryDecodeFilterFromHash() ?? tryLoadFilterFromLocalStorage() ?? DEFAULT_FILTER,
  );
  const [config, setConfig] = useState<SearchConfig>(
    tryLoadConfigFromLocalStorage() ?? DEFAULT_CONFIG,
  );

  const [isRunning, setIsRunning] = useState(false);
  const [isPaused, setIsPaused] = useState(false);
  const [isDone, setIsDone] = useState(false);

  const [matches, setMatches] = useState<MatchRecord[]>([]);
  const [seedsPerSec, setSeedsPerSec] = useState(0);
  const [totalScanned, setTotalScanned] = useState(0n);
  const [workerCount, setWorkerCount] = useState(0);

  const [engineStatus, setEngineStatus] = useState<EngineStatus>('detecting');

  // ─── V3 (WebGPU) beta state ──────────────────────────────────────────────
  // Off by default. ?v3=1 reveals the toggle for everyone. Once enabled the
  // user keeps it across reloads via localStorage. Searches still run on
  // WASM in V3 — see docs/V3_DESIGN.md.
  const showV3Toggle = (() => {
    try {
      const p = new URLSearchParams(window.location.search);
      return p.get('v3') === '1' || localStorage.getItem('seed-searcher-v3-beta') === 'on';
    } catch { return false; }
  })();
  const [v3Beta, setV3Beta] = useState<boolean>(() => {
    try { return localStorage.getItem('seed-searcher-v3-beta') === 'on'; }
    catch { return false; }
  });
  useEffect(() => {
    try { localStorage.setItem('seed-searcher-v3-beta', v3Beta ? 'on' : 'off'); } catch {}
  }, [v3Beta]);
  const [v3Status, setV3Status] = useState<string>('');

  const orchestratorRef = useRef<SearchOrchestrator | null>(null);

  // Detect SIMD on mount
  useEffect(() => {
    void detectSimd().then(simd => setEngineStatus(simd ? 'simd' : 'scalar'));
  }, []);

  // ─── Cold-start pre-warm (phase 2) ─────────────────────────────────────────
  //
  // On every mount, fire off a low-priority fetch of the engine .wasm so the
  // browser parks it in HTTP cache and the WebAssembly module compile cache
  // before the user clicks Start. On the APK this trims ~200-400ms off the
  // first-search latency on a cold launch.
  //
  // We deliberately do NOT spawn the worker yet — the worker spawn itself is
  // cheap, and pre-spawning would burn battery if the user just opened the
  // app to browse jokers.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      const simd = await detectSimd();
      const canThread =
        typeof crossOriginIsolated !== 'undefined' && crossOriginIsolated === true
        && typeof SharedArrayBuffer !== 'undefined';
      const urls: string[] = [];
      if (canThread) urls.push(assetPath('engine-threads/balatro_seed_engine_bg.wasm'));
      urls.push(simd ? assetPath('engine-simd/balatro_seed_engine_bg.wasm') : assetPath('engine/balatro_seed_engine_bg.wasm'));
      for (const url of urls) {
        if (cancelled) return;
        try {
          // Plain fetch warms the HTTP cache. `WebAssembly.compileStreaming`
          // additionally warms the module-compile cache in Chromium/Safari.
          const res = await fetch(url, { cache: 'force-cache' });
          if (cancelled || !res.ok) return;
          if (typeof WebAssembly.compileStreaming === 'function') {
            await WebAssembly.compileStreaming(res);
          }
        } catch {
          // Best-effort; failures are silent (the real load will surface them).
        }
      }
    })();
    return () => { cancelled = true; };
  }, []);

  // V3 probe — runs whenever the beta toggle flips on.
  useEffect(() => {
    let cancelled = false;
    if (!v3Beta) { setV3Status(''); return; }
    setV3Status('probing…');
    (async () => {
      try {
        const simd = await detectSimd();
        const basePath = simd ? 'engine-simd' : 'engine';
        // Use runtime URL construction so Vite doesn't try to bundle the
        // public asset at build time.
        const origin = window.location.origin;
        const jsUrl = new URL(assetPath(`${basePath}/balatro_seed_engine.js`), origin).toString();
        const wasmUrl = new URL(assetPath(`${basePath}/balatro_seed_engine_bg.wasm`), origin).toString();
        const wasmJs = await import(/* @vite-ignore */ jsUrl) as {
          default: (opts?: { module_or_path?: string }) => Promise<unknown>;
          v3_diagnostic_cpu: (a: number, b: number, c: number) => Uint32Array;
          v3_diagnostic_shader_source: () => string;
        };
        await wasmJs.default({ module_or_path: wasmUrl });
        const { selectEngine } = await import('./v3/engineSelector');
        const desc = await selectEngine({
          v3Beta: true,
          wasm: {
            v3_diagnostic_cpu: wasmJs.v3_diagnostic_cpu,
            v3_diagnostic_shader_source: wasmJs.v3_diagnostic_shader_source,
          },
        });
        if (cancelled) return;
        if (desc.webgpu?.kind === 'ready') {
          const mb = (desc.webgpu.throughputSeedsPerSec / 1e6).toFixed(1);
          setV3Status(`WebGPU verified · ${desc.webgpu.adapterInfo} · ~${mb}M ops/s diagnostic`);
        } else if (desc.webgpu?.kind === 'unsupported') {
          setV3Status(`WebGPU unavailable: ${desc.webgpu.reason}`);
        } else if (desc.webgpu?.kind === 'verification-failed') {
          setV3Status(`WebGPU verification failed: ${desc.webgpu.reason}`);
        } else {
          setV3Status('probing…');
        }
      } catch (e) {
        if (!cancelled) setV3Status(`V3 probe error: ${String(e)}`);
      }
    })();
    return () => { cancelled = true; };
  }, [v3Beta]);

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

  // Persist filter to URL hash AND localStorage. The hash is for sharing,
  // localStorage is for survivability across APK WebView evictions.
  useEffect(() => {
    try {
      const json = JSON.stringify(filter);
      const encoded = btoa(unescape(encodeURIComponent(json)));
      history.replaceState(null, '', `#filter=${encoded}`);
      localStorage.setItem(LS_FILTER_KEY, json);
    } catch {
      // ignore
    }
  }, [filter]);

  // Persist config (deck/stake/seedLen/topN) to localStorage only — not part
  // of the shareable URL because users likely want the deck they're playing.
  useEffect(() => {
    try {
      localStorage.setItem(LS_CONFIG_KEY, JSON.stringify(config));
    } catch {
      // ignore
    }
  }, [config]);

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
            {showV3Toggle && v3Beta && (
              <span
                className="engine-pill"
                style={{
                  background: 'rgba(8, 145, 178, 0.15)',
                  color: '#67e8f9',
                  border: '1px solid rgba(8, 145, 178, 0.4)',
                }}
                title={v3Status}
              >
                V3 beta
              </span>
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

            {showV3Toggle && (
              <div
                className="card"
                style={{
                  borderColor: 'rgba(8, 145, 178, 0.4)',
                  background: 'rgba(8, 145, 178, 0.06)',
                }}
              >
                <h2 className="card-title">V3 engine (WebGPU beta)</h2>
                <label
                  className="config-row"
                  style={{ display: 'flex', alignItems: 'flex-start', gap: 8, cursor: 'pointer' }}
                >
                  <input
                    type="checkbox"
                    checked={v3Beta}
                    onChange={e => setV3Beta(e.target.checked)}
                    disabled={isRunning}
                    style={{ marginTop: 4 }}
                  />
                  <span style={{ fontSize: 12, lineHeight: 1.4, color: '#cbd5e1' }}>
                    Probe your GPU and run a verified integer benchmark.{' '}
                    <strong style={{ color: '#67e8f9' }}>Searches still run on WASM</strong> in
                    this build — the GPU search path is blocked on full f64 emulation.
                    See V3_DESIGN.md.
                  </span>
                </label>
                {v3Beta && v3Status && (
                  <div
                    style={{
                      marginTop: 6,
                      fontFamily: 'ui-monospace, SFMono-Regular, monospace',
                      fontSize: 10,
                      color: '#67e8f9',
                      wordBreak: 'break-word',
                    }}
                  >
                    {v3Status}
                  </div>
                )}
              </div>
            )}
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
            href="https://github.com/JupiterianSoul/Balatro-Seed-Searcher"
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
