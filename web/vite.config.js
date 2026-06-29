import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
const crossOriginHeaders = {
    'Cross-Origin-Opener-Policy': 'same-origin',
    'Cross-Origin-Embedder-Policy': 'require-corp',
};
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
