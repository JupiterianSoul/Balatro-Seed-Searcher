import { useCallback, useRef, useState } from 'react';
import type { MatchRecord, Filter } from '../types';

// ─── Helpers ─────────────────────────────────────────────────────────────────

function formatNumber(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(Math.round(n));
}

function formatBigInt(n: bigint): string {
  if (n >= 1_000_000_000n) return `${Number(n / 1_000_000n) / 1000}B`;
  if (n >= 1_000_000n) return `${Number(n / 1_000n) / 1000}M`;
  if (n >= 1_000n) return `${Number(n)}`.replace(/\B(?=(\d{3})+(?!\d))/g, ',');
  return String(n);
}

function encodeFilterToHash(filter: Filter): string {
  try {
    const json = JSON.stringify(filter);
    return btoa(unescape(encodeURIComponent(json)));
  } catch {
    return '';
  }
}

// ─── ScoreBar ─────────────────────────────────────────────────────────────────

function ScoreBar({ score }: { score: number }) {
  const pct = Math.min(100, (score / 10) * 100);
  return (
    <div className="score-bar-wrap" title={`Score: ${score}`}>
      <div className="score-bar" style={{ width: `${pct}%` }} />
      <span className="score-label">{score}</span>
    </div>
  );
}

// ─── CopyButton ──────────────────────────────────────────────────────────────

function CopyButton({ text, label }: { text: string; label?: string }) {
  const [copied, setCopied] = useState(false);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const handleClick = useCallback(() => {
    void navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      if (timerRef.current) clearTimeout(timerRef.current);
      timerRef.current = setTimeout(() => setCopied(false), 1500);
    });
  }, [text]);

  return (
    <button
      className={`btn btn--copy ${copied ? 'btn--copied' : ''}`}
      onClick={handleClick}
      aria-label={`Copy ${label ?? text}`}
      title="Copy to clipboard"
    >
      {copied ? '✓' : '⎘'}
    </button>
  );
}

// ─── ResultsPanel props ──────────────────────────────────────────────────────

export type ResultsPanelProps = {
  matches: MatchRecord[];
  seedsPerSec: number;
  workerCount: number;
  totalScanned: bigint;
  isRunning: boolean;
  isPaused: boolean;
  isDone: boolean;
  filter: Filter;
  onStart: () => void;
  onStop: () => void;
  onPause: () => void;
};

// ─── ResultsPanel component ──────────────────────────────────────────────────

export function ResultsPanel({
  matches,
  seedsPerSec,
  workerCount,
  totalScanned,
  isRunning,
  isPaused,
  isDone,
  filter,
  onStart,
  onStop,
  onPause,
}: ResultsPanelProps) {
  const [filterUrlCopied, setFilterUrlCopied] = useState(false);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const shareFilter = useCallback(() => {
    const hash = encodeFilterToHash(filter);
    const url = `${window.location.origin}${window.location.pathname}#filter=${hash}`;
    void navigator.clipboard.writeText(url).then(() => {
      setFilterUrlCopied(true);
      if (timerRef.current) clearTimeout(timerRef.current);
      timerRef.current = setTimeout(() => setFilterUrlCopied(false), 2000);
    });
  }, [filter]);

  return (
    <div className="results-panel card">
      {/* Stats bar */}
      <div className="results-stats card-header">
        <h2 className="card-title">Results</h2>
        <div className="stats-chips">
          <span className="chip">
            <strong>{formatNumber(seedsPerSec)}</strong> seeds/sec
          </span>
          <span className="chip">
            <strong>{workerCount}</strong> worker{workerCount !== 1 ? 's' : ''}
          </span>
          <span className="chip">
            <strong>{formatBigInt(totalScanned)}</strong> searched
          </span>
          <span className="chip chip--accent">
            <strong>{matches.length}</strong> match{matches.length !== 1 ? 'es' : ''}
          </span>
        </div>
      </div>

      {/* Control buttons */}
      <div className="results-controls">
        {!isRunning && !isPaused && (
          <button className="btn btn--primary" onClick={onStart} disabled={isRunning}>
            {isDone ? '↺ Search again' : '▶ Start search'}
          </button>
        )}
        {isRunning && (
          <>
            <button className="btn btn--secondary" onClick={onPause}>
              ⏸ Pause
            </button>
            <button className="btn btn--danger-outline" onClick={onStop}>
              ■ Stop
            </button>
          </>
        )}
        {isPaused && (
          <>
            <button className="btn btn--primary" onClick={onStart}>
              ▶ Resume
            </button>
            <button className="btn btn--danger-outline" onClick={onStop}>
              ■ Stop
            </button>
          </>
        )}
        {isDone && !isRunning && !isPaused && (
          <span className="status-badge status-badge--done">Done</span>
        )}
        {isRunning && (
          <span className="status-badge status-badge--running">
            <span className="pulse-dot" /> Scanning…
          </span>
        )}
        <button
          className={`btn btn--ghost ${filterUrlCopied ? 'btn--copied' : ''}`}
          onClick={shareFilter}
          title="Copy a shareable URL with the current filter"
        >
          {filterUrlCopied ? '✓ Copied!' : '⎘ Share filter URL'}
        </button>
      </div>

      {/* Results table */}
      <div className="results-table-wrap">
        {matches.length === 0 ? (
          <div className="empty-results">
            {isRunning
              ? 'Scanning… matches will appear here.'
              : 'No matches yet. Configure a filter and start searching.'}
          </div>
        ) : (
          <table className="results-table">
            <thead>
              <tr>
                <th className="col-rank">#</th>
                <th className="col-seed">Seed</th>
                <th className="col-score">Score</th>
                <th className="col-actions">Actions</th>
              </tr>
            </thead>
            <tbody>
              {matches.map((m, i) => (
                <tr key={String(m.rank)} className="result-row">
                  <td className="col-rank result-rank">{i + 1}</td>
                  <td className="col-seed">
                    <span className="seed-cell">{m.seed}</span>
                  </td>
                  <td className="col-score">
                    <ScoreBar score={m.score} />
                  </td>
                  <td className="col-actions">
                    <CopyButton text={m.seed} label="seed" />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}
