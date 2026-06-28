# Balatro Seed Searcher — Web Frontend

A Vite + React + TypeScript UI for the Balatro seed search engine.

## Development

```bash
npm install
npm run dev
```

The dev server starts at `http://localhost:5173`.

## Build

```bash
npm run build
```

Output goes to `dist/`.

## Preview production build

```bash
npm run preview
```

## Type-check only

```bash
npm run typecheck
```

## COOP / COEP headers

This app requires `SharedArrayBuffer` for the WASM search engine. Both the dev
server and preview server automatically set:

```
Cross-Origin-Opener-Policy: same-origin
Cross-Origin-Embedder-Policy: require-corp
```

If you deploy behind a reverse proxy (nginx, Caddy, etc.), you must forward
these headers or set them explicitly in your server config, otherwise the WASM
worker will fail to initialise.
