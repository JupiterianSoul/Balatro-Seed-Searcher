// Feature-detect WASM SIMD and load the appropriate engine bundle.
// The SIMD detection bytes encode a minimal WASM module that uses the i8x16.splat SIMD instruction.

const SIMD_TEST_BYTES = new Uint8Array([
  0x00, 0x61, 0x73, 0x6d, // magic: \0asm
  0x01, 0x00, 0x00, 0x00, // version: 1
  0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7b, // type section: () -> v128
  0x03, 0x02, 0x01, 0x00, // function section
  0x0a, 0x0a, 0x01, 0x08, 0x00, 0x41, 0x00, 0xfd, 0x0f, 0xfd, 0x62, 0x0b, // code section
]);

function detectSimd(): boolean {
  try {
    return WebAssembly.validate(SIMD_TEST_BYTES);
  } catch {
    return false;
  }
}

export type EngineModule = {
  init: () => void;
  scan_chunk: (
    filter_json: string,
    start_rank: bigint,
    count: bigint,
    seed_len: number,
    deck_idx: number,
    stake_idx: number,
    partial: boolean,
    min_score: number,
  ) => Uint8Array;
};

export type LoadedEngine = {
  engine: EngineModule;
  simd: boolean;
};

let cached: LoadedEngine | null = null;

export async function loadEngine(): Promise<LoadedEngine> {
  if (cached) return cached;

  const simd = detectSimd();
  const basePath = simd ? '/engine-simd' : '/engine';
  const scriptUrl = `${basePath}/balatro_seed_engine.js`;

  // Dynamically import the ES module. The WASM loader's default export
  // is an async init function; named exports include the actual API.
  const mod = await import(/* @vite-ignore */ scriptUrl) as {
    default: (opts?: { module_or_path?: string }) => Promise<unknown>;
    init: () => void;
    scan_chunk: (
      filter_json: string,
      start_rank: bigint,
      count: bigint,
      seed_len: number,
      deck_idx: number,
      stake_idx: number,
      partial: boolean,
      min_score: number,
    ) => Uint8Array;
  };

  // Initialise the WASM binary (fetches the .wasm from same directory)
  await mod.default({ module_or_path: `${basePath}/balatro_seed_engine_bg.wasm` });

  // Call the Rust init function to seed the random tables
  mod.init();

  const engine: EngineModule = {
    init: mod.init,
    scan_chunk: mod.scan_chunk,
  };

  cached = { engine, simd };
  return cached;
}
