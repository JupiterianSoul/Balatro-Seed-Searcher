// Engine selector for the standalone web app's V3 beta path.
//
// V3 (when the user enables the beta toggle):
//   1. Tries the WebGPU diagnostic verification.
//      - If verified, the UI shows a "WebGPU verified" pill alongside
//        the active WASM backend. Searches still run on WASM because
//        the GPU search shader isn't ported yet (see V3_DESIGN.md).
//      - If verification fails, falls back to V2 WASM-SIMD/scalar.
//
// V2 path (default, V3 toggle off): unchanged — direct WebAssembly
// with SIMD detection done by `engine/loader.ts`.

import { probeWebGpu, type V3GpuStatus, type V3WasmModule } from './webgpuEngine';

export type EngineKind = 'webgpu+wasm-simd' | 'wasm-simd' | 'wasm-scalar';

export type EngineDescriptor = {
  kind: EngineKind;
  label: string;
  /**
   * Source of compute for actual searches. V3 currently always uses
   * WASM. The WebGPU stack is verified end-to-end but doesn't yet run
   * the search workload — this changes once the f64-emulated GPU
   * pipeline lands.
   */
  searchBackend: 'wasm';
  webgpu?: V3GpuStatus;
};

const SIMD_TEST_BYTES = new Uint8Array([
  0x00, 0x61, 0x73, 0x6d,
  0x01, 0x00, 0x00, 0x00,
  0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7b,
  0x03, 0x02, 0x01, 0x00,
  0x0a, 0x0a, 0x01, 0x08, 0x00, 0x41, 0x00, 0xfd, 0x0f, 0xfd, 0x62, 0x0b,
]);

function detectSimd(): boolean {
  try { return WebAssembly.validate(SIMD_TEST_BYTES); } catch { return false; }
}

export async function selectEngine(opts: {
  v3Beta: boolean;
  wasm?: V3WasmModule | null;
}): Promise<EngineDescriptor> {
  const simd = detectSimd();
  const wasmKind: EngineKind = simd ? 'wasm-simd' : 'wasm-scalar';
  const wasmLabel = simd ? 'WASM SIMD' : 'WASM scalar';

  if (!opts.v3Beta) {
    return { kind: wasmKind, label: wasmLabel, searchBackend: 'wasm' };
  }

  if (!opts.wasm) {
    return { kind: wasmKind, label: `${wasmLabel} · V3 verify pending`, searchBackend: 'wasm' };
  }

  const status = await probeWebGpu(opts.wasm);
  if (status.kind === 'ready') {
    return {
      kind: 'webgpu+wasm-simd',
      label: `WebGPU verified (${status.adapterInfo}) · ${wasmLabel}`,
      searchBackend: 'wasm',
      webgpu: status,
    };
  }
  return {
    kind: wasmKind,
    label: `${wasmLabel} · V3 fell back (${status.kind === 'unsupported' ? status.reason : status.kind})`,
    searchBackend: 'wasm',
    webgpu: status,
  };
}
