// Resolve a public asset path against Vite's configured base URL.
//
// At dev time and on a root-served deploy (pplx.app subdomain, file://, APK)
// `import.meta.env.BASE_URL` is "/" so this is identity.
// On GitHub Pages where the site is hosted under /Balatro-Seed-Searcher/,
// Vite is configured with that base and this helper rewrites every engine
// fetch to the correct subpath.
//
// Always pass a path *without* a leading slash (e.g. "engine-simd/foo.wasm").

export function assetPath(relative: string): string {
  const base = (import.meta.env.BASE_URL ?? '/').replace(/\/?$/, '/');
  const rel = relative.replace(/^\/+/, '');
  return base + rel;
}
