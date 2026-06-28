import React from 'react';

function runSearch(): void {
  console.log('TODO');
}

export default function App(): React.JSX.Element {
  return (
    <div
      style={{
        minHeight: '100vh',
        background: '#0a0a0f',
        color: '#f5f5f7',
        fontFamily:
          '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Oxygen, Ubuntu, Cantarell, sans-serif',
        display: 'flex',
        flexDirection: 'column',
        alignItems: 'center',
        justifyContent: 'center',
        gap: '1rem',
      }}
    >
      <h1 style={{ margin: 0, fontSize: '2.5rem', fontWeight: 700 }}>
        Balatro Seed Searcher
      </h1>
      <p style={{ margin: 0, fontSize: '1.125rem', opacity: 0.7 }}>
        The fastest Balatro seed search engine on the web.
      </p>
      <button
        onClick={runSearch}
        style={{
          marginTop: '1rem',
          padding: '0.75rem 2rem',
          fontSize: '1rem',
          fontWeight: 600,
          background: '#e63946',
          color: '#f5f5f7',
          border: 'none',
          borderRadius: '0.5rem',
          cursor: 'pointer',
        }}
      >
        Run search
      </button>
    </div>
  );
}
