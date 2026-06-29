//! Balatro Seed Engine — core RNG + seed-state derivations.
//!
//! Ported from MathIsFun0/Immolate (OpenCL reference) and the documented LuaJIT
//! RNG that the Balatro game itself uses. This crate compiles to:
//!   - native (for benchmarks, tests, and the optional cloud worker)
//!   - wasm32-unknown-unknown (scalar)
//!   - wasm32-unknown-unknown + `-C target-feature=+simd128` (browser SIMD)
//!
//! The public surface is intentionally thin: a `Searcher` that scans the
//! base-36 seed space against a compiled filter and reports matches.

pub mod tables;
pub mod rng;
pub mod seed;
pub mod state;
pub mod instance;
pub mod items;
pub mod derive;
pub mod filter;
pub mod search;
pub mod v3;

#[cfg(target_arch = "wasm32")]
pub mod wasm_api;

pub use rng::{pseudohash, LuaRandom};
pub use seed::Seed;
pub use instance::{Instance, NodeKey, RandomType, RandomSource};
pub use search::{Searcher, Match};
