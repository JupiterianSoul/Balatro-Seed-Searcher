// V3 WebGPU engine — diagnostic + capability layer (standalone web).
//
// Mirrors `Balatropedia/client/src/lib/v3/webgpuEngine.ts`. Detects WebGPU,
// compiles the diagnostic shader (sourced from the WASM-exposed
// `v3_diagnostic_shader_source`), dispatches it, then verifies the GPU
// output bit-for-bit against the WASM CPU reference (`v3_diagnostic_cpu`).
//
// V3 deliberately does NOT yet run real seed searches on the GPU.
// See `docs/V3_DESIGN.md` for why (f64-precision blocker in pseudohash +
// LuaRandom). V3 ships as plumbing + a verified integer benchmark, so
// users can see real GPU throughput on their hardware and so the next
// sprint slots full-precision math into the same scaffold.

export type V3GpuStatus =
  | { kind: 'unsupported'; reason: string }
  | { kind: 'detecting' }
  | { kind: 'ready'; adapterInfo: string; throughputSeedsPerSec: number }
  | { kind: 'verification-failed'; reason: string };

export type V3WasmModule = {
  v3_diagnostic_cpu: (seed_base: number, iter_count: number, seed_count: number) => Uint32Array;
  v3_diagnostic_shader_source: () => string;
};

const WORKGROUP_SIZE = 64;
const DEFAULT_SEED_COUNT = 4096;
const DEFAULT_ITER_COUNT = 200;

/**
 * Detect, initialise, and verify the WebGPU stack. Returns a status
 * object the UI can display directly. Safe to call repeatedly.
 */
export async function probeWebGpu(wasm: V3WasmModule): Promise<V3GpuStatus> {
  if (typeof navigator === 'undefined' || !('gpu' in navigator)) {
    return { kind: 'unsupported', reason: 'navigator.gpu missing (browser or WebView too old)' };
  }
  const gpu = (navigator as Navigator & { gpu?: GPU }).gpu;
  if (!gpu) return { kind: 'unsupported', reason: 'navigator.gpu is undefined' };

  let adapter: GPUAdapter | null;
  try {
    adapter = await gpu.requestAdapter();
  } catch (e) {
    return { kind: 'unsupported', reason: `requestAdapter threw: ${String(e)}` };
  }
  if (!adapter) {
    return { kind: 'unsupported', reason: 'no compatible GPU adapter' };
  }

  let device: GPUDevice;
  try {
    device = await adapter.requestDevice();
  } catch (e) {
    return { kind: 'unsupported', reason: `requestDevice threw: ${String(e)}` };
  }

  const adapterInfo = await formatAdapterInfo(adapter);

  const shaderSource = wasm.v3_diagnostic_shader_source();
  let module: GPUShaderModule;
  try {
    module = device.createShaderModule({ code: shaderSource });
  } catch (e) {
    return { kind: 'verification-failed', reason: `shader compile failed: ${String(e)}` };
  }

  const compInfo = await module.getCompilationInfo();
  const errors = compInfo.messages.filter(m => m.type === 'error');
  if (errors.length > 0) {
    const msg = errors.map(e => e.message).join('; ');
    return { kind: 'verification-failed', reason: `shader compile errors: ${msg}` };
  }

  const pipeline = device.createComputePipeline({
    layout: 'auto',
    compute: { module, entryPoint: 'benchmark' },
  });

  const seedCount = DEFAULT_SEED_COUNT;
  const iterCount = DEFAULT_ITER_COUNT;
  const seedBase = 42;

  const paramsBuffer = device.createBuffer({
    size: 16,
    usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
  });
  device.queue.writeBuffer(paramsBuffer, 0, new Uint32Array([seedBase, iterCount, seedCount, 0]));

  const resultsBuffer = device.createBuffer({
    size: seedCount * 4,
    usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC,
  });

  const readbackBuffer = device.createBuffer({
    size: seedCount * 4,
    usage: GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST,
  });

  const bindGroup = device.createBindGroup({
    layout: pipeline.getBindGroupLayout(0),
    entries: [
      { binding: 0, resource: { buffer: paramsBuffer } },
      { binding: 1, resource: { buffer: resultsBuffer } },
    ],
  });

  const workgroupCount = Math.ceil(seedCount / WORKGROUP_SIZE);
  const t0 = performance.now();

  const encoder = device.createCommandEncoder();
  const pass = encoder.beginComputePass();
  pass.setPipeline(pipeline);
  pass.setBindGroup(0, bindGroup);
  pass.dispatchWorkgroups(workgroupCount);
  pass.end();
  encoder.copyBufferToBuffer(resultsBuffer, 0, readbackBuffer, 0, seedCount * 4);
  device.queue.submit([encoder.finish()]);

  await readbackBuffer.mapAsync(GPUMapMode.READ);
  const gpuResults = new Uint32Array(readbackBuffer.getMappedRange().slice(0));
  readbackBuffer.unmap();

  const gpuMs = performance.now() - t0;

  const cpuResults = wasm.v3_diagnostic_cpu(seedBase, iterCount, seedCount);
  let mismatches = 0;
  for (let i = 0; i < seedCount; i++) {
    if (gpuResults[i] !== cpuResults[i]) mismatches++;
  }

  if (mismatches > 0) {
    return {
      kind: 'verification-failed',
      reason: `GPU output diverges from WASM reference (${mismatches}/${seedCount} mismatches). Driver bug or shader incompatibility.`,
    };
  }

  const equivalentSeeds = seedCount * iterCount;
  const throughputSeedsPerSec = (equivalentSeeds * 1000) / Math.max(gpuMs, 1);

  return {
    kind: 'ready',
    adapterInfo,
    throughputSeedsPerSec,
  };
}

async function formatAdapterInfo(adapter: GPUAdapter): Promise<string> {
  try {
    const anyAdapter = adapter as GPUAdapter & {
      requestAdapterInfo?: () => Promise<GPUAdapterInfo>;
      info?: GPUAdapterInfo;
    };
    const info = anyAdapter.info ?? (anyAdapter.requestAdapterInfo
      ? await anyAdapter.requestAdapterInfo()
      : null);
    if (!info) return 'unknown adapter';
    const parts = [info.vendor, info.architecture, info.device]
      .filter(s => s && s.length > 0);
    return parts.length ? parts.join(' / ') : 'unknown adapter';
  } catch {
    return 'unknown adapter';
  }
}
