//! V3 engine: WebGPU-targeted reference implementations.
//!
//! Contains:
//!   - `df`: double-float (two-f32) arithmetic; bit-identical to what the
//!     WGSL shader runs.
//!   - `pseudohash_df`: f64-equivalent pseudohash implemented in DF f32.
//!   - `lua_random_df`: f64-equivalent LuaRandom in DF f32.
//!   - `boss_probe_df`: a self-contained boss probe used to validate parity.
//!
//! These are intentionally written as a *reference* for the WGSL shader,
//! NOT for production CPU use. The CPU side keeps using `crate::rng`
//! (native f64) which is faster and bit-identical to the game.

pub mod df;
pub mod pseudohash;
pub mod lua_random;
pub mod boss_probe;
pub mod diagnostic;
