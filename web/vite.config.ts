import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

const crossOriginHeaders = {
  'Cross-Origin-Opener-Policy': 'same-origin',
  'Cross-Origin-Embedder-Policy': 'require-corp',
};

// `BASE` is set by the GitHub Pages workflow to "/Balatro-Seed-Searcher/" so
// the built assets reference the right subpath. For the pplx.app subdomain
// build and local dev it stays at "/".
const base = process.env.BASE ?? '/';

export default defineConfig({
  base,
  plugins: [react()],
  optimizeDeps: {
    exclude: ['balatro-seed-engine'],
  },
  server: {
    headers: crossOriginHeaders,
  },
  preview: {
    headers: crossOriginHeaders,
  },
});
