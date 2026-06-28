/// <reference types="@webgpu/types" />
// WebGPU compute path — experimental, opt-in.
//
// Strategy: dispatch one workgroup per 64-seed block. Each thread computes
// pseudohash for one seed against a single "key prefix" (e.g. "Boss" + ante).
// Threads write a 1-bit pass/fail back to a storage buffer; the main thread
// reads, filters, and emits matches.
//
// Honest scope:
// - Current shader implements ONE clause (boss check) as proof-of-concept.
//   The CPU/WASM path remains the canonical multi-clause evaluator.
// - When this is stable, more clause kinds get added as additional shaders
//   (one per RandomType) and the host fuses them by &&ing pass-bits.
// - Not all browsers expose WebGPU yet; the loader auto-falls back to WASM.

export type GpuSupport =
  | { ok: true; adapter: GPUAdapter; device: GPUDevice }
  | { ok: false; reason: string };

export async function detectWebGpu(): Promise<GpuSupport> {
  if (typeof navigator === 'undefined' || !('gpu' in navigator)) {
    return { ok: false, reason: 'navigator.gpu not present' };
  }
  try {
    const adapter = await (navigator as Navigator & { gpu: GPU }).gpu.requestAdapter();
    if (!adapter) return { ok: false, reason: 'no GPU adapter' };
    const device = await adapter.requestDevice();
    return { ok: true, adapter, device };
  } catch (e) {
    return { ok: false, reason: String(e) };
  }
}

// Minimal WGSL pseudohash + boss-clause kernel. The full multi-source
// engine lands incrementally; this is the proof of the compute path.
export const PSEUDOHASH_KERNEL_WGSL = /* wgsl */ `
// Balatro pseudohash — port of the Immolate reference, but f32 instead of
// f64 (WebGPU baseline doesn't require f64). This loses determinism vs the
// CPU engine and is therefore used as a PRE-FILTER only: GPU rejects most
// seeds, CPU re-checks survivors with f64 precision.

struct Params {
    seed_count: u32,
    start_rank: u32,
    key_len: u32,
    threshold_x1000: u32,  // multiply by 1e-3 for the f32 threshold
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> key_bytes: array<u32>;  // u8 unpacked to u32
@group(0) @binding(2) var<storage, read_write> pass_bits: array<u32>;

// Base-35 alphabet: '1'..'9','A'..'Z' (skipping '0' and 'O').
fn seed_char(idx: u32) -> u32 {
    if (idx < 9u) { return 49u + idx; }            // '1'..'9'
    var c = 65u + (idx - 9u);                       // 'A'..
    if (c >= 79u) { c = c + 1u; }                   // skip 'O'
    return c;
}

fn write_seed(buf: ptr<function, array<u32, 16>>, rank: u32, len: u32) {
    var r = rank;
    var i: i32 = i32(len) - 1;
    loop {
        if (i < 0) { break; }
        (*buf)[i] = seed_char(r % 35u);
        r = r / 35u;
        i = i - 1;
    }
}

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let id = gid.x;
    if (id >= params.seed_count) { return; }
    let rank = params.start_rank + id;

    // Build "key + seed" into a stack buffer (max key 8 + seed 8 = 16).
    var buf: array<u32, 16>;
    for (var i: u32 = 0u; i < params.key_len; i = i + 1u) {
        buf[i] = key_bytes[i];
    }
    write_seed(&buf, rank, 8u);
    let total_len = params.key_len + 8u;

    // pseudohash with f32. Diverges from the f64 CPU path past ~10 sig figs.
    var num: f32 = 1.0;
    let shift: f32 = f32(1u << 16u) * f32(1u << 16u);   // 2^32
    let pi: f32 = 3.14159265;
    let inv_const: f32 = 1.1239285023;

    var i: i32 = i32(total_len) - 1;
    loop {
        if (i < 0) { break; }
        let ch = f32(buf[u32(i)]);
        let a = inv_const / num * ch * pi;
        let b = pi * (f32(i) + 1.0);
        let scaled = (a + b) * shift;
        let intp = floor(scaled);
        let fa = (a * shift) - floor(a * shift);
        let fb = (b * shift) - floor(b * shift);
        let combined = fa + fb;
        let fract_part = combined - floor(combined);
        let sum = (intp + fract_part) / shift;
        num = sum - floor(sum);
        i = i - 1;
    }

    // Threshold passed → set the bit.
    let threshold = f32(params.threshold_x1000) * 0.001;
    if (num > threshold) {
        let word_idx = id / 32u;
        let bit_idx = id % 32u;
        let mask = 1u << bit_idx;
        atomicOr(&pass_bits[word_idx], mask);
        _ = mask;  // suppress unused if atomic not available
    }
}
`;

export interface GpuPipeline {
  device: GPUDevice;
  pipeline: GPUComputePipeline;
  paramBuffer: GPUBuffer;
  keyBuffer: GPUBuffer;
  passBuffer: GPUBuffer;
  readbackBuffer: GPUBuffer;
}

export async function createPipeline(support: Extract<GpuSupport, { ok: true }>): Promise<GpuPipeline> {
  const { device } = support;
  const module = device.createShaderModule({ code: PSEUDOHASH_KERNEL_WGSL });
  const pipeline = device.createComputePipeline({ layout: 'auto', compute: { module, entryPoint: 'main' } });

  const paramBuffer = device.createBuffer({ size: 16, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
  const keyBuffer = device.createBuffer({ size: 64, usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST });
  // 65536 seeds → 2048 u32s of pass bits
  const passBuffer = device.createBuffer({ size: 65536 / 8, usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC });
  const readbackBuffer = device.createBuffer({ size: 65536 / 8, usage: GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST });

  return { device, pipeline, paramBuffer, keyBuffer, passBuffer, readbackBuffer };
}

// Dispatch one batch of `count` seeds starting at `startRank`. Returns the
// indices (0..count) that passed the threshold — caller resolves them to
// real seeds and re-validates with the f64 CPU engine.
export async function gpuScan(
  pipe: GpuPipeline,
  startRank: number,
  count: number,
  keyPrefix: string,
  thresholdX1000: number,
): Promise<Uint32Array> {
  const { device, pipeline, paramBuffer, keyBuffer, passBuffer, readbackBuffer } = pipe;

  // Upload key bytes unpacked as u32 (lazy but correct)
  const keyBytes = new Uint32Array(16);
  for (let i = 0; i < keyPrefix.length; i++) keyBytes[i] = keyPrefix.charCodeAt(i);
  device.queue.writeBuffer(keyBuffer, 0, keyBytes);

  const params = new Uint32Array([count, startRank, keyPrefix.length, thresholdX1000]);
  device.queue.writeBuffer(paramBuffer, 0, params);

  const bindGroup = device.createBindGroup({
    layout: pipeline.getBindGroupLayout(0),
    entries: [
      { binding: 0, resource: { buffer: paramBuffer } },
      { binding: 1, resource: { buffer: keyBuffer } },
      { binding: 2, resource: { buffer: passBuffer } },
    ],
  });

  const encoder = device.createCommandEncoder();
  encoder.clearBuffer(passBuffer);
  const pass = encoder.beginComputePass();
  pass.setPipeline(pipeline);
  pass.setBindGroup(0, bindGroup);
  pass.dispatchWorkgroups(Math.ceil(count / 64));
  pass.end();
  encoder.copyBufferToBuffer(passBuffer, 0, readbackBuffer, 0, 65536 / 8);
  device.queue.submit([encoder.finish()]);

  await readbackBuffer.mapAsync(GPUMapMode.READ);
  const passBits = new Uint32Array(readbackBuffer.getMappedRange().slice(0));
  readbackBuffer.unmap();

  // Unpack bits → indices
  const survivors: number[] = [];
  for (let word = 0; word < passBits.length; word++) {
    let bits = passBits[word];
    while (bits) {
      const bit = bits & -bits;
      const idx = (word * 32) + Math.log2(bit);
      if (idx < count) survivors.push(idx);
      bits ^= bit;
    }
  }
  return new Uint32Array(survivors);
}
