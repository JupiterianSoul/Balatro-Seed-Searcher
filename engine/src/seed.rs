//! Balatro seed string encoding + iteration.
//!
//! Balatro seeds are 1-8 chars from a 35-symbol alphabet (no 0, no O — the
//! "looks-like-zero" exclusions). Total space for 8-char seeds: 35^8 ≈ 2.25e12.

pub const SEED_CHARS: &[u8; 35] = b"123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
pub const NUM_CHARS: usize = 35;

/// A Balatro seed represented as up to 8 alphabet indices (0..35).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seed {
    data: [u8; 8],
    len: u8,
}

impl Seed {
    /// Construct from a string. Returns `None` if any char is invalid.
    pub fn parse(s: &str) -> Option<Self> {
        let bytes = s.as_bytes();
        if bytes.is_empty() || bytes.len() > 8 {
            return None;
        }
        let mut data = [0u8; 8];
        for (i, &b) in bytes.iter().enumerate() {
            let idx = SEED_CHARS.iter().position(|&c| c == b)?;
            data[i] = idx as u8;
        }
        Some(Seed { data, len: bytes.len() as u8 })
    }

    /// Construct from an integer rank (0-indexed) in the base-35 ordering.
    /// Lower ranks have shorter prefixes ("1", "2", ..., "11", "12", ...).
    pub fn from_rank(mut rank: u64, len: u8) -> Self {
        debug_assert!(len > 0 && len <= 8);
        let mut data = [0u8; 8];
        for i in (0..len as usize).rev() {
            data[i] = (rank % NUM_CHARS as u64) as u8;
            rank /= NUM_CHARS as u64;
        }
        Seed { data, len }
    }

    /// Reverse of `from_rank`.
    pub fn rank(&self) -> u64 {
        let mut acc: u64 = 0;
        let mut mult: u64 = 1;
        for i in (0..self.len as usize).rev() {
            acc += self.data[i] as u64 * mult;
            mult *= NUM_CHARS as u64;
        }
        acc
    }

    pub fn as_str(&self) -> heapless::String<8> {
        let mut s = heapless::String::new();
        for i in 0..self.len as usize {
            let _ = s.push(SEED_CHARS[self.data[i] as usize] as char);
        }
        s
    }

    /// Stack-allocated rendering for the hot path — avoids any heap traffic.
    #[inline]
    pub fn write_to(&self, buf: &mut [u8; 8]) -> usize {
        for i in 0..self.len as usize {
            buf[i] = SEED_CHARS[self.data[i] as usize];
        }
        self.len as usize
    }

    /// Bump to the next seed in lexicographic alphabet order, growing the
    /// length if all positions wrap.
    #[inline]
    pub fn increment(&mut self) {
        let mut i = self.len as isize - 1;
        loop {
            if i < 0 {
                // grew past current length
                self.len += 1;
                self.data[(self.len - 1) as usize] = 0;
                // shift existing zeros right (we already filled with zeros)
                return;
            }
            self.data[i as usize] += 1;
            if (self.data[i as usize] as usize) < NUM_CHARS {
                return;
            }
            self.data[i as usize] = 0;
            i -= 1;
        }
    }
}

// Tiny no-std string type for the hot path. Pulled inline so we don't add a
// dep just for an 8-char buffer.
mod heapless {
    use core::fmt;

    #[derive(Clone, Copy)]
    pub struct String<const N: usize> {
        buf: [u8; 8],
        len: u8,
    }
    impl<const N: usize> String<N> {
        pub fn new() -> Self { Self { buf: [0; 8], len: 0 } }
        pub fn push(&mut self, c: char) -> Result<(), ()> {
            if self.len as usize >= N { return Err(()); }
            self.buf[self.len as usize] = c as u8;
            self.len += 1;
            Ok(())
        }
        pub fn as_str(&self) -> &str {
            // SAFETY: only ASCII-alphabet bytes are written via `push`.
            unsafe { core::str::from_utf8_unchecked(&self.buf[..self.len as usize]) }
        }
    }
    impl<const N: usize> fmt::Display for String<N> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(self.as_str())
        }
    }
}

pub use heapless::String as HeaplessSeedString;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let s = Seed::parse("PHRAYJUS").unwrap();
        assert_eq!(s.as_str().as_str(), "PHRAYJUS");
    }

    #[test]
    fn increment_wraps() {
        let mut s = Seed::parse("Z").unwrap();
        s.increment();
        assert_eq!(s.as_str().as_str(), "11");
    }

    #[test]
    fn rank_round_trip() {
        for rank in [0u64, 1, 34, 35, 100, 1_000_000] {
            let s = Seed::from_rank(rank, 6);
            assert_eq!(s.rank(), rank, "rank round-trip broke at {rank}");
        }
    }
}
