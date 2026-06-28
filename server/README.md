# Balatro Seed Searcher — Optional Cloud-Boost Server

> **This is an OPTIONAL cloud-boost adapter.**
> The seed searcher works fully offline in your browser without this server.
> You do not need to run this at all for normal use.

## What it does

This is a tiny Node.js WebSocket server that accepts seed-scan requests from
the web UI and runs them using a native engine binary (compiled from the
`engine/` Rust crate). Because it runs natively on the server's CPU rather
than inside the browser's WebAssembly sandbox, it can be faster for large
searches.

## How to run it

```bash
cd server
npm install
npm start
```

The server starts on **port 8787** by default.

To use a different port:

```bash
PORT=9000 npm start
```

For development with auto-restart on file changes:

```bash
npm run dev
```

## Pointing the web UI at the server

In the web UI settings (or via the `cloudServerUrl` option), enter:

```
ws://localhost:8787
```

or, if running on a remote host:

```
ws://your-server-hostname:8787
```

The UI will fall back to its built-in WASM engine automatically if the
WebSocket connection fails or is not configured.

## Performance expectations (honest)

| Environment | Speedup vs browser WASM |
|---|---|
| Same desktop machine | ~1–2× (marginal) |
| Free-tier cloud (Render / Fly / Railway) | ~3–5× over a phone; ~1–2× over a desktop |

> **Not worth paying for** unless you regularly search for sub-1-in-1B
> criteria. For everyday use the browser WASM engine is fast enough.

## Configuration

| Environment variable | Default | Description |
|---|---|---|
| `PORT` | `8787` | WebSocket listen port |
| `MAX_WORKERS` | `4` | Max concurrent scan workers |

## Protocol

The server speaks a simple JSON WebSocket protocol:

**Client → Server**

```json
{
  "type": "scan",
  "startRank": 0,
  "count": 10000,
  "filterJson": "{}",
  "seedLen": 8,
  "deckIdx": 0,
  "stakeIdx": 0,
  "partial": false,
  "minScore": 0
}
```

**Server → Client**

| Message | Meaning |
|---|---|
| `{"type":"match","seed":"ABCD1234","rank":42,"score":87}` | A seed that passed filters |
| `{"type":"progress","scanned":500}` | Heartbeat every ~100 ms |
| `{"type":"done","scanned":10000}` | Chunk finished |
| `{"type":"error","msg":"..."}` | Something went wrong |

## Native engine

The server tries to load the native engine binary at:

```
../engine/target/release/balatro-seed-engine
```

If it is not built yet, the server will respond with an error message but
remain running. The `src/worker.js` stub emits placeholder data in the
meantime so the protocol can be tested end-to-end.

To build the engine (requires Rust / Cargo):

```bash
cd engine
cargo build --release
```

## Security

**This server has no authentication.** It accepts connections from anyone
who can reach its port.

- **Local use only:** run it on `localhost` and do not expose port 8787 to
  the internet.
- **Remote use:** put it behind your own reverse proxy with TLS and an auth
  layer (e.g. Nginx + HTTP Basic Auth, Cloudflare Access, Tailscale, etc.)
  before exposing it publicly.

## Docker

A minimal Dockerfile is included for containerised deployment:

```bash
docker build -t balatro-seed-server .
docker run -p 8787:8787 balatro-seed-server
```
