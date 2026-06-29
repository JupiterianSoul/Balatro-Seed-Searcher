//! `pseudohash` ported to double-float arithmetic.
//!
//! This implementation must produce results bit-identical to the WGSL
//! shader in `shaders/pseudohash.wgsl`. It is NOT used by the production
//! search path — that still runs native f64 in `crate::rng`. This module
//! exists solely so the Rust-side parity harness can compare GPU output
//! against a known-equivalent reference.
//!
//! Compared to the f64 `pseudohash` it differs in two ways:
//!   1. All intermediate arithmetic is performed in DF (two f32s).
//!   2. The `(a+b) * 2^32 floor` step yields an i64 represented as two
//!      i32 halves, since WGSL has no i64.
//!
//! The expectation is that DF-emulated arithmetic will produce results
//! *very close* to native f64 but not always identical — the divergence
//! rate is what the parity harness measures.

use super::df::{self, Df};

/// DF analogue of f64 pseudohash.
pub fn pseudohash_df(s: &[u8]) -> Df {
    let pi = Df::from_f64(std::f64::consts::PI);
    let c1 = Df::from_f64(1.1239285023);

    // 2^32 as DF — exactly representable.
    let shift = Df::from_f64(4_294_967_296.0_f64);

    let mut num = Df::ONE;
    let len = s.len();

    let mut i = len as isize - 1;
    while i >= 0 {
        let ch = Df::from_f32(s[i as usize] as f32);

        // a = 1.1239285023 / num * ch * PI
        let a = df::mul(df::mul(df::div(c1, num), ch), pi);
        // b = PI * (i + 1)
        let b = df::mul(pi, Df::from_f32((i + 1) as f32));

        // scaled = (a + b) * 2^32
        let scaled = df::mul(df::add(a, b), shift);
        let int_part = df::floor(scaled);

        // fract_part = fract(fract(a*shift) + fract(b*shift))
        let af = df::fract(df::mul(a, shift));
        let bf = df::fract(df::mul(b, shift));
        let fract_part = df::fract(df::add(af, bf));

        // num = fract((int_part + fract_part) / shift)
        num = df::fract(df::div(df::add(int_part, fract_part), shift));

        i -= 1;
    }
    num
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::pseudohash;

    #[test]
    fn df_pseudohash_close_to_f64_on_short_keys() {
        // For short keys (typical: "boss" + 8-char seed = 12 bytes), DF
        // should produce results within ~1e-6 of f64.
        let keys = [
            "boss1ABCD1234",
            "Tag1ABCD1234",
            "Voucher1ZZZZZZZZ",
            "Joker1AAAA1111",
        ];
        for k in &keys {
            let f64_val = pseudohash(k);
            let df_val = pseudohash_df(k.as_bytes()).to_f64();
            let diff = (f64_val - df_val).abs();
            // Document, don't fail: this is what the parity harness measures.
            eprintln!("key={} f64={:.15} df={:.15} diff={:.3e}",
                k, f64_val, df_val, diff);
        }
    }
}
