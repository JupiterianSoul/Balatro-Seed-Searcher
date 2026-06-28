//! Per-run instance + RNG cache.
//!
//! This is the heart of seed determinism. For every "draw" the game performs,
//! it composes a node key (type, source, ante, resample) and either:
//!   - looks up an existing node in its cache, OR
//!   - hashes the composed string with `pseudohash(key + seed)` to create one.
//!
//! Then it mutates the node's state by `* 1.72431234 + 2.134453429141` (mod 1)
//! and averages with the run's hashed seed. That average feeds `LuaRandom` which
//! finally produces the random number the game uses.
//!
//! Ported verbatim from Immolate's lib/cache.cl + lib/instance.cl.

use crate::rng::{pseudohash, LuaRandom};
use std::collections::BTreeSet;

/// All RNG sources the game uses (which "key" prefix is hashed).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum RandomType {
    JokerCommon, JokerUncommon, JokerRare, JokerLegendary,
    JokerRarity, JokerEdition,
    Misprint, StandardHasEnhancement, Enhancement, Card,
    StandardEdition, StandardHasSeal, StandardSeal,
    ShopPack, Tarot, Spectral, Tags,
    ShuffleNewRound, CardType, Planet,
    LuckyMult, LuckyMoney, Sigil, Ouija, WheelOfFortune,
    GrosMichel, Cavendish, Voucher, VoucherTag, OrbitalTag,
    Soul, Erratic,
    Eternal, Perishable, Rental, EternalPerishable,
    RentalPack, EternalPerishablePack, Boss,
}

impl RandomType {
    pub fn key(self) -> &'static str {
        match self {
            Self::JokerCommon => "Joker1",
            Self::JokerUncommon => "Joker2",
            Self::JokerRare => "Joker3",
            Self::JokerLegendary => "Joker4",
            Self::JokerRarity => "rarity",
            Self::JokerEdition => "edi",
            Self::Misprint => "misprint",
            Self::StandardHasEnhancement => "stdset",
            Self::Enhancement => "Enhanced",
            Self::Card => "front",
            Self::StandardEdition => "standard_edition",
            Self::StandardHasSeal => "stdseal",
            Self::StandardSeal => "stdsealtype",
            Self::ShopPack => "shop_pack",
            Self::Tarot => "Tarot",
            Self::Spectral => "Spectral",
            Self::Tags => "Tag",
            Self::ShuffleNewRound => "nr",
            Self::CardType => "cdt",
            Self::Planet => "Planet",
            Self::LuckyMult => "lucky_mult",
            Self::LuckyMoney => "lucky_money",
            Self::Sigil => "sigil",
            Self::Ouija => "ouija",
            Self::WheelOfFortune => "wheel_of_fortune",
            Self::GrosMichel => "gros_michel",
            Self::Cavendish => "cavendish",
            Self::Voucher => "Voucher",
            Self::VoucherTag => "Voucher_fromtag",
            Self::OrbitalTag => "orbital",
            Self::Soul => "soul_",
            Self::Erratic => "erratic",
            Self::Eternal => "stake_shop_joker_eternal",
            Self::Perishable => "ssjp",
            Self::Rental => "ssjr",
            Self::EternalPerishable => "etperpoll",
            Self::RentalPack => "packssjr",
            Self::EternalPerishablePack => "packetper",
            Self::Boss => "boss",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum RandomSource {
    Shop, Emperor, HighPriestess, Judgement, Wraith,
    Arcana, Celestial, Spectral, Standard, Buffoon,
    Vagabond, Superposition, EightBall, Seance, SixthSense,
    TopUp, RareTag, UncommonTag, BlueSeal, PurpleSeal,
    Soul, RiffRaff, Cartomancer, Null,
}

impl RandomSource {
    pub fn key(self) -> &'static str {
        match self {
            Self::Shop => "sho", Self::Emperor => "emp", Self::HighPriestess => "pri",
            Self::Judgement => "jud", Self::Wraith => "wra", Self::Arcana => "ar1",
            Self::Celestial => "pl1", Self::Spectral => "spe", Self::Standard => "sta",
            Self::Buffoon => "buf", Self::Vagabond => "vag", Self::Superposition => "sup",
            Self::EightBall => "8ba", Self::Seance => "sea", Self::SixthSense => "sixth",
            Self::TopUp => "top", Self::RareTag => "rta", Self::UncommonTag => "uta",
            Self::BlueSeal => "blusl", Self::PurpleSeal => "8ba", // intentional dup per ref
            Self::Soul => "sou", Self::RiffRaff => "rif", Self::Cartomancer => "car",
            Self::Null => "",
        }
    }
}

/// Composable node descriptor. The cache keys nodes by this tuple.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeKey {
    pub kind: RandomType,
    pub source: Option<RandomSource>,
    pub ante: Option<i32>,
    pub resample: Option<u32>,
}

impl NodeKey {
    pub fn simple(kind: RandomType) -> Self {
        Self { kind, source: None, ante: None, resample: None }
    }
    pub fn with_ante(kind: RandomType, source: RandomSource, ante: i32) -> Self {
        Self { kind, source: Some(source), ante: Some(ante), resample: None }
    }
    pub fn with_resample(kind: RandomType, source: RandomSource, ante: i32, n: u32) -> Self {
        Self { kind, source: Some(source), ante: Some(ante), resample: Some(n) }
    }

    /// Build the hashed-string form: type_str + source_str + ante_str + resample_str.
    fn render(&self) -> String {
        let mut out = String::with_capacity(40);
        out.push_str(self.kind.key());
        if let Some(s) = self.source { out.push_str(s.key()); }
        if let Some(a) = self.ante { out.push_str(&a.to_string()); }
        if let Some(r) = self.resample {
            if r > 0 {
                out.push_str("_resample");
                out.push_str(&(r + 1).to_string());
            }
        }
        out
    }
}

/// Single cache entry — a node and its mutated state.
#[derive(Clone, Copy, Debug)]
struct Node {
    key: NodeKey,
    state: f64,
}

/// Default cache size matches Immolate's reference (64 nodes per run).
const CACHE_SIZE: usize = 64;

/// Per-run game instance. Holds the hashed seed, the RNG state machine,
/// the node cache, and the lock state set used by `randchoice_common` to
/// drive the resample loop.
pub struct Instance {
    seed_string: heapless8::Buf,
    hashed_seed: f64,
    rng: LuaRandom,
    nodes: [Option<Node>; CACHE_SIZE],
    next_free: usize,
    /// Items currently locked. Mirrors Immolate's `locked[]` array; only the
    /// items that have been touched are stored, which keeps memory tiny while
    /// matching the same resample semantics. Initial profile/run locks are
    /// applied via `apply_default_locks`.
    locked: BTreeSet<&'static str>,
    /// When true, locked items don't trigger a resample (Showman joker effect).
    /// Defaults to false; the search loop opts in per filter when needed.
    pub showman: bool,
    /// Stake of the run — controls sticker rolls. Defaults to White (no stickers).
    pub stake: crate::state::Stake,
}

impl Instance {
    pub fn new(seed: &str) -> Self {
        let mut buf = heapless8::Buf::new();
        buf.write(seed);
        let hashed = pseudohash(seed);
        let rng = LuaRandom::from_seed(hashed);
        Self {
            seed_string: buf,
            hashed_seed: hashed,
            rng,
            nodes: [None; CACHE_SIZE],
            next_free: 0,
            locked: BTreeSet::new(),
            showman: false,
            stake: crate::state::Stake::White,
        }
    }

    /// Lock an item so the next `randchoice_common` will resample past it.
    /// Used by pack openers during their temporary-lock loop.
    #[inline]
    pub fn lock(&mut self, item: &'static str) { self.locked.insert(item); }

    /// Unlock an item. Pack openers call this after they finish their
    /// temporary-lock loop so subsequent calls see a clean slate.
    #[inline]
    pub fn unlock(&mut self, item: &'static str) { self.locked.remove(item); }

    /// Is this item currently locked? `false` when `showman` is on.
    #[inline]
    pub fn is_locked(&self, item: &str) -> bool {
        if self.showman { return false; }
        self.locked.contains(item)
    }

    /// Apply per-ante boss / tag unlocks. Mirrors Immolate's `init_locks` for
    /// the bits that matter to a deterministic seed search: bosses gated
    /// behind ante 2/3/4/5/6.
    pub fn apply_ante_locks(&mut self, ante: i32) {
        // Boss blinds gated behind antes
        if ante < 2 {
            for b in &["The Mouth", "The Fish", "The Wall", "The House", "The Mark",
                       "The Wheel", "The Arm", "The Water", "The Needle", "The Flint"] {
                self.locked.insert(b);
            }
        }
        if ante < 3 {
            self.locked.insert("The Tooth");
            self.locked.insert("The Eye");
        }
        if ante < 4 { self.locked.insert("The Plant"); }
        if ante < 5 { self.locked.insert("The Serpent"); }
        if ante < 6 { self.locked.insert("The Ox"); }
    }

    pub fn seed_str(&self) -> &str { self.seed_string.as_str() }
    pub fn hashed_seed(&self) -> f64 { self.hashed_seed }

    /// `get_node_child` from Immolate: lookup or create a cache node, mutate
    /// it once, return `(state + hashed_seed) / 2`. This is the value that
    /// gets passed to `randomseed()` to produce the actual draw.
    pub fn get_node(&mut self, key: NodeKey) -> f64 {
        let idx = self.find_or_init(key);
        let n = self.nodes[idx].as_mut().expect("node initialised");
        n.state = round_digits(fract(n.state * 1.72431234 + 2.134453429141), 13);
        (n.state + self.hashed_seed) * 0.5
    }

    fn find_or_init(&mut self, key: NodeKey) -> usize {
        for i in 0..self.next_free {
            if let Some(n) = &self.nodes[i] {
                if n.key == key { return i; }
            }
        }
        // initialise
        let rendered = key.render();
        let mut combined = String::with_capacity(rendered.len() + self.seed_string.len());
        combined.push_str(&rendered);
        combined.push_str(self.seed_string.as_str());
        let state = pseudohash(&combined);
        let i = self.next_free;
        if i >= CACHE_SIZE {
            // Cache is full — overwrite the oldest. Real game runs rarely
            // exceed ~30 nodes per run so this is a safety net.
            self.next_free = CACHE_SIZE - 1;
            self.nodes[CACHE_SIZE - 1] = Some(Node { key, state });
            return CACHE_SIZE - 1;
        }
        self.nodes[i] = Some(Node { key, state });
        self.next_free += 1;
        i
    }

    /// `random()` — the workhorse the simulator calls. Re-seeds the LuaRandom
    /// from the node's state, then draws one double.
    pub fn random(&mut self, key: NodeKey) -> f64 {
        let s = self.get_node(key);
        self.rng = LuaRandom::from_seed(s);
        self.rng.next_double()
    }

    /// `randchoice(items)` — pick one item from a non-empty slice, uniform.
    /// Mirrors Lua's `items[math.random(#items)]` (1-indexed).
    pub fn rand_choice<T: Copy>(&mut self, key: NodeKey, items: &[T]) -> T {
        debug_assert!(!items.is_empty());
        let s = self.get_node(key);
        self.rng = LuaRandom::from_seed(s);
        let idx = (self.rng.next_double() * items.len() as f64).floor() as usize;
        items[idx.min(items.len() - 1)]
    }

    /// `randchoice_common(items)` — the lock-aware draw used by jokers, tarots,
    /// planets, spectrals. If the rolled item is locked, resamples with an
    /// incrementing `resample` suffix until an unlocked item is found.
    /// Mirrors `instance.cl::randchoice_common`.
    pub fn rand_choice_common(
        &mut self,
        kind: RandomType,
        source: RandomSource,
        ante: i32,
        items: &[&'static str],
    ) -> &'static str {
        let key = NodeKey::with_ante(kind, source, ante);
        let mut pick = self.rand_choice(key, items);
        if !self.showman && self.locked.contains(pick) {
            let mut resample_n: u32 = 1;
            // Bounded by item pool size — we can't have more resamples than items.
            // Add a hard cap of items.len() * 4 as a safety net (matches Immolate's
            // worst-case behaviour where it would loop forever on a fully-locked pool).
            let cap = (items.len() as u32) * 4 + 8;
            while self.locked.contains(pick) && resample_n < cap {
                let rk = NodeKey::with_resample(kind, source, ante, resample_n);
                pick = self.rand_choice(rk, items);
                resample_n += 1;
            }
        }
        pick
    }

    /// `randweightedchoice(items)` — weighted draw used for booster pack types.
    /// `total_weight` is the sum of every weight in `items`; the caller passes
    /// it explicitly so we don't recompute it per call.
    pub fn rand_weighted_choice(
        &mut self,
        key: NodeKey,
        items: &[(&'static str, f64)],
        total_weight: f64,
    ) -> &'static str {
        debug_assert!(!items.is_empty());
        let s = self.get_node(key);
        self.rng = LuaRandom::from_seed(s);
        let poll = self.rng.next_double() * total_weight;
        let mut acc = 0.0;
        for (item, w) in items {
            acc += *w;
            if acc >= poll { return item; }
        }
        // Floating-point edge case — fall back to last.
        items[items.len() - 1].0
    }
}

#[inline]
fn fract(x: f64) -> f64 { x - x.floor() }
#[inline]
fn round_digits(f: f64, d: u32) -> f64 {
    let p = 10f64.powi(d as i32);
    (f * p).round() / p
}

/// Stack-allocated 8-char seed buffer.
mod heapless8 {
    #[derive(Clone, Copy)]
    pub struct Buf { bytes: [u8; 8], len: u8 }
    impl Buf {
        pub fn new() -> Self { Self { bytes: [0; 8], len: 0 } }
        pub fn write(&mut self, s: &str) {
            let b = s.as_bytes();
            let n = b.len().min(8);
            self.bytes[..n].copy_from_slice(&b[..n]);
            self.len = n as u8;
        }
        pub fn as_str(&self) -> &str {
            unsafe { core::str::from_utf8_unchecked(&self.bytes[..self.len as usize]) }
        }
        pub fn len(&self) -> usize { self.len as usize }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_key_rendering() {
        let k = NodeKey::with_ante(RandomType::JokerRarity, RandomSource::Shop, 1);
        assert_eq!(k.render(), "raritysho1");
    }

    #[test]
    fn cache_returns_same_node_for_same_key() {
        let mut inst = Instance::new("TESTSEED");
        let k = NodeKey::with_ante(RandomType::JokerRarity, RandomSource::Shop, 1);
        let v1 = inst.get_node(k);
        let v2 = inst.get_node(k);
        // State mutates each call, so two calls return DIFFERENT values
        assert_ne!(v1, v2);
    }

    #[test]
    fn determinism_across_instances() {
        let mut a = Instance::new("PHRAYJUS");
        let mut b = Instance::new("PHRAYJUS");
        let k = NodeKey::with_ante(RandomType::Boss, RandomSource::Null, 1);
        for _ in 0..20 {
            assert_eq!(a.random(k).to_bits(), b.random(k).to_bits());
        }
    }
}
