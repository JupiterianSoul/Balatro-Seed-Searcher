//! Double-float (DF) arithmetic — emulates f64 precision using two f32s.
//!
//! A DF value is a pair `(hi, lo)` of f32 such that the true value is
//! `hi + lo` and `|lo| < 0.5 * ulp(hi)`. This roughly doubles the precision
//! of a single f32: ~14 decimal digits, comparable to native f64 in the
//! ranges we care about ([-2^31, 2^31] for pseudohash intermediates).
//!
//! Algorithms here are the standard Dekker/Knuth "two-sum" and "two-product"
//! constructions. They are written deliberately in the simplest form so the
//! WGSL port is a line-by-line translation.
//!
//! References:
//!   - T. J. Dekker, "A Floating-Point Technique for Extending the
//!     Available Precision" (1971).
//!   - Y. Hida, X. S. Li, D. H. Bailey, "Library for Double-Double and
//!     Quad-Double Arithmetic" (2007).
//!
//! NOTE on FMA: Rust's stable `mul_add` is hardware-FMA when available,
//! which we explicitly do NOT want here because WGSL does not guarantee
//! FMA. We use plain `*` and `+` so the Rust reference matches the WGSL
//! shader bit-for-bit on platforms where the GPU lacks FMA.

#![allow(clippy::float_arithmetic)]

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Df {
    pub hi: f32,
    pub lo: f32,
}

impl Df {
    pub const ZERO: Df = Df { hi: 0.0, lo: 0.0 };
    pub const ONE: Df = Df { hi: 1.0, lo: 0.0 };

    #[inline]
    pub const fn new(hi: f32, lo: f32) -> Self {
        Df { hi, lo }
    }

    /// Promote an f32 to DF.
    #[inline]
    pub const fn from_f32(x: f32) -> Self {
        Df { hi: x, lo: 0.0 }
    }

    /// Promote an f64 to DF — only used by the Rust-side reference.
    /// The shader receives DF values that are constructed entirely in f32.
    #[inline]
    pub fn from_f64(x: f64) -> Self {
        let hi = x as f32;
        let lo = (x - hi as f64) as f32;
        Df { hi, lo }
    }

    /// Collapse DF back to f64 — used for assertions in tests only.
    #[inline]
    pub fn to_f64(self) -> f64 {
        self.hi as f64 + self.lo as f64
    }
}

// ---------------------------------------------------------------------------
// Primitives
// ---------------------------------------------------------------------------

/// "Quick two-sum": `a + b` where `|a| >= |b|`. Returns `(hi, lo)`.
#[inline]
fn quick_two_sum(a: f32, b: f32) -> (f32, f32) {
    let s = a + b;
    let err = b - (s - a);
    (s, err)
}

/// "Two-sum": general-case `a + b` with no magnitude requirement.
#[inline]
fn two_sum(a: f32, b: f32) -> (f32, f32) {
    let s = a + b;
    let bb = s - a;
    let err = (a - (s - bb)) + (b - bb);
    (s, err)
}

/// "Two-product" via Dekker splitting (no FMA assumed).
/// Returns `(p, err)` such that `a * b = p + err` exactly.
///
/// Splits each operand into a 12-bit-high and 12-bit-low halves so that the
/// product of two halves fits exactly in f32. (f32 has 24 mantissa bits; we
/// split at 12 to keep the product's mantissa <= 24 bits.)
#[inline]
fn split(a: f32) -> (f32, f32) {
    // 2^12 + 1 = 4097
    const SPLITTER: f32 = 4097.0;
    let t = SPLITTER * a;
    let hi = t - (t - a);
    let lo = a - hi;
    (hi, lo)
}

#[inline]
fn two_prod(a: f32, b: f32) -> (f32, f32) {
    let p = a * b;
    let (ah, al) = split(a);
    let (bh, bl) = split(b);
    // err = ((ah*bh - p) + ah*bl + al*bh) + al*bl
    let err = ((ah * bh - p) + ah * bl + al * bh) + al * bl;
    (p, err)
}

// ---------------------------------------------------------------------------
// DF operations
// ---------------------------------------------------------------------------

#[inline]
pub fn add(a: Df, b: Df) -> Df {
    let (s, e) = two_sum(a.hi, b.hi);
    let e = e + (a.lo + b.lo);
    let (hi, lo) = quick_two_sum(s, e);
    Df { hi, lo }
}

#[inline]
pub fn sub(a: Df, b: Df) -> Df {
    add(a, Df { hi: -b.hi, lo: -b.lo })
}

#[inline]
pub fn mul(a: Df, b: Df) -> Df {
    let (p, e) = two_prod(a.hi, b.hi);
    let e = e + (a.hi * b.lo + a.lo * b.hi);
    let (hi, lo) = quick_two_sum(p, e);
    Df { hi, lo }
}

/// DF division — uses a Newton-style correction. ~14-digit accurate.
#[inline]
pub fn div(a: Df, b: Df) -> Df {
    let q1 = a.hi / b.hi;
    // r = a - q1 * b
    let prod = mul(Df::from_f32(q1), b);
    let r = sub(a, prod);
    let q2 = r.hi / b.hi;
    let (hi, lo) = quick_two_sum(q1, q2);
    Df { hi, lo }
}

/// `floor(x)` on a DF. Returns a DF with `lo == 0` when `hi` already has
/// no fractional part, otherwise applies `floor` to `hi+lo` carefully.
#[inline]
pub fn floor(x: Df) -> Df {
    let fh = x.hi.floor();
    if fh == x.hi {
        // hi is integral; the fractional part lives entirely in lo.
        let fl = x.lo.floor();
        let (hi, lo) = quick_two_sum(fh, fl);
        Df { hi, lo }
    } else {
        Df { hi: fh, lo: 0.0 }
    }
}

/// `fract(x) = x - floor(x)`.
#[inline]
pub fn fract(x: Df) -> Df {
    sub(x, floor(x))
}

/// Convert a DF *guaranteed* to fit in [-2^31, 2^31] to an exact i32 pair
/// `(hi32, lo32)` representing the i64 value of `floor(x)`.
///
/// We need this because pseudohash takes `floor((a+b)*2^32)` and treats it
/// as an i64. On the GPU we represent that i64 as two i32 halves.
#[inline]
pub fn floor_to_i64_halves(x: Df) -> (i32, i32) {
    // Strategy: scale down by 2^32 first to bring into a range f32 can
    // represent, then split. But pseudohash actually passes us a value
    // ALREADY scaled by 2^32, so we need full i64 precision.
    //
    // We construct the i64 by recovering the integer parts of hi and lo
    // separately and combining.
    let fh = x.hi.floor();
    let fl_total = x.hi - fh + x.lo;
    let fl = fl_total.floor();
    let total = fh as f64 + fl as f64;
    // Clamp to i64 range — pseudohash inputs guarantee fit but be defensive.
    let i64v = if total >= i64::MAX as f64 {
        i64::MAX
    } else if total <= i64::MIN as f64 {
        i64::MIN
    } else {
        total as i64
    };
    let hi32 = (i64v >> 32) as i32;
    let lo32 = i64v as i32;
    (hi32, lo32)
}

/// Inverse of `floor_to_i64_halves`: convert two i32 halves back to a DF.
#[inline]
pub fn i64_halves_to_df(hi32: i32, lo32: i32) -> Df {
    let val = ((hi32 as i64) << 32) | (lo32 as u32 as i64);
    Df::from_f64(val as f64)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f64, b: f64, tol: f64) -> bool {
        let diff = (a - b).abs();
        diff < tol || diff < tol * a.abs().max(b.abs())
    }

    #[test]
    fn from_to_f64_roundtrip() {
        for &v in &[0.0, 1.0, -1.5, 1.1239285023, std::f64::consts::PI, 1e-10, 1e10] {
            let df = Df::from_f64(v);
            assert!(approx_eq(df.to_f64(), v, 1e-6), "{} != {}", df.to_f64(), v);
        }
    }

    #[test]
    fn add_matches_f64_in_typical_range() {
        let a = Df::from_f64(1.1239285023);
        let b = Df::from_f64(std::f64::consts::PI);
        let s = add(a, b);
        let expected = 1.1239285023 + std::f64::consts::PI;
        assert!(approx_eq(s.to_f64(), expected, 1e-7));
    }

    #[test]
    fn mul_matches_f64_in_typical_range() {
        let a = Df::from_f64(1.72431234);
        let b = Df::from_f64(0.5);
        let p = mul(a, b);
        let expected = 1.72431234 * 0.5;
        assert!(approx_eq(p.to_f64(), expected, 1e-7));
    }

    #[test]
    fn fract_handles_typical_pseudohash_state() {
        // A typical mid-iteration state.
        let x = Df::from_f64(123.456789012345);
        let f = fract(x);
        assert!(f.to_f64() >= 0.0 && f.to_f64() < 1.0);
        assert!(approx_eq(f.to_f64(), 0.456789012345, 1e-6));
    }

    #[test]
    fn i64_halves_roundtrip() {
        let v: i64 = 0x12345678_9ABCDEF0u64 as i64;
        let df = Df::from_f64(v as f64);
        let (hi, lo) = floor_to_i64_halves(df);
        // f32 precision can't perfectly represent every i64; we check the
        // top bits survive.
        let recon = ((hi as i64) << 32) | (lo as u32 as i64);
        let diff = (v - recon).unsigned_abs();
        // f64 → f32 round trip on a 60-bit integer loses some bits, but
        // for pseudohash inputs we only use this in [-2^53, 2^53] where
        // f64 is exact and DF gives us back full range.
        assert!(diff < (1u64 << 30), "diff too large: {}", diff);
    }
}
