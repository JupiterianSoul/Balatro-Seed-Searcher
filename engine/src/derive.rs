//! Real game-state derivations. Ported from Immolate's functions.cl.
//!
//! Every function here takes a fresh `Instance` (so the node cache starts
//! empty) and returns the canonical "what is this draw" answer. Callers
//! typically build one `Instance` per (seed, run config) and then ask it
//! multiple questions — the cache makes the second-and-later calls cheap.
//!
//! Implemented now (most-queried surface):
//!   - `next_joker_rarity` — common/uncommon/rare/legendary roll
//!   - `next_joker_edition` — base/foil/holo/poly/negative roll
//!   - `next_joker` — full shop joker draw (rarity + identity, no stickers)
//!   - `next_tag` — skip-tag draw
//!   - `next_voucher` — per-ante voucher draw
//!   - `next_boss` — boss blind draw (small/big bosses for ante%8!=0,
//!     finisher bosses for ante%8==0)
//!
//! Stickers, locking, voucher chain side-effects, and pack contents land in
//! a follow-up — they need the lock/voucher state machine to be honest.

use crate::instance::{Instance, NodeKey, RandomSource, RandomType};
use crate::tables::{
    COMMON_JOKERS, LEGENDARY_JOKERS, RARE_JOKERS, UNCOMMON_JOKERS, TAGS, VOUCHERS,
    BOSSES_SMALL_BIG, BOSSES_FINISHER, TAROTS, PLANETS, SPECTRALS,
};
use crate::state::Stake;

/// Rarity bucket the next joker draw falls into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rarity { Common, Uncommon, Rare, Legendary }

/// Edition rolled for the next joker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edition { None, Foil, Holographic, Polychrome, Negative }

/// Mirrors `functions.cl::next_joker_rarity`.
pub fn next_joker_rarity(inst: &mut Instance, source: RandomSource, ante: i32) -> Rarity {
    match source {
        RandomSource::Soul => return Rarity::Legendary,
        // Wraith spectral always yields a Rare joker
        RandomSource::Null => {} // fall through to roll
        _ => {}
    }
    // The static-by-source forced rarities:
    if matches!(source, RandomSource::Soul) { return Rarity::Legendary; }
    if matches!(source, RandomSource::RareTag) { return Rarity::Rare; }
    if matches!(source, RandomSource::UncommonTag) { return Rarity::Uncommon; }
    if matches!(source, RandomSource::RiffRaff) { return Rarity::Common; }

    let key = NodeKey {
        kind: RandomType::JokerRarity,
        source: Some(source),
        ante: Some(ante),
        resample: None,
    };
    let roll = inst.random(key);
    if roll > 0.95 { Rarity::Rare }
    else if roll > 0.7 { Rarity::Uncommon }
    else { Rarity::Common }
}

/// Mirrors `functions.cl::next_joker_edition`.
pub fn next_joker_edition(inst: &mut Instance, source: RandomSource, ante: i32) -> Edition {
    let key = NodeKey::with_ante(RandomType::JokerEdition, source, ante);
    let poll = inst.random(key);
    if poll > 0.997 { Edition::Negative }
    else if poll > 0.994 { Edition::Polychrome }
    else if poll > 0.98 { Edition::Holographic }
    else if poll > 0.96 { Edition::Foil }
    else { Edition::None }
}

/// Mirrors `functions.cl::next_joker` (identity only).
/// Now lock-aware — calls `rand_choice_common` so a locked item triggers
/// the resample loop (matters inside pack openers and showman-off runs).
pub fn next_joker(inst: &mut Instance, source: RandomSource, ante: i32) -> &'static str {
    let rarity = next_joker_rarity(inst, source, ante);
    let pool: &[&'static str] = match rarity {
        Rarity::Common => COMMON_JOKERS,
        Rarity::Uncommon => UNCOMMON_JOKERS,
        Rarity::Rare => RARE_JOKERS,
        Rarity::Legendary => LEGENDARY_JOKERS,
    };
    let kind = match rarity {
        Rarity::Common => RandomType::JokerCommon,
        Rarity::Uncommon => RandomType::JokerUncommon,
        Rarity::Rare => RandomType::JokerRare,
        Rarity::Legendary => RandomType::JokerLegendary,
    };
    inst.rand_choice_common(kind, source, ante, pool)
}

/// Sticker bundle returned alongside a joker draw.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stickers {
    pub eternal: bool,
    pub perishable: bool,
    pub rental: bool,
}

const ETERNAL_BLACKLIST: &[&str] = &[
    "Cavendish", "Diet Cola", "Gros Michel", "Ice Cream", "Invisible Joker",
    "Luchador", "Mr. Bones", "Popcorn", "Ramen", "Seltzer", "Turtle Bean",
];
const PERISHABLE_BLACKLIST: &[&str] = &[
    "Castle", "Ceremonial Dagger", "Constellation", "Flash Card",
    "Green Joker", "Lucky Cat", "Madness", "Obelisk", "Red Card",
    "Ride the Bus", "Rocket", "Runner", "Spare Trousers", "Square Joker",
    "Vampire", "Wee Joker",
];

/// Joker with rarity, edition, and sticker rolls. Mirrors
/// `functions.cl::next_joker_with_info`.
pub fn next_joker_with_stickers(
    inst: &mut Instance, source: RandomSource, ante: i32,
) -> (&'static str, Rarity, Edition, Stickers) {
    let rarity = next_joker_rarity(inst, source, ante);
    let joker = {
        let pool: &[&'static str] = match rarity {
            Rarity::Common => COMMON_JOKERS,
            Rarity::Uncommon => UNCOMMON_JOKERS,
            Rarity::Rare => RARE_JOKERS,
            Rarity::Legendary => LEGENDARY_JOKERS,
        };
        let kind = match rarity {
            Rarity::Common => RandomType::JokerCommon,
            Rarity::Uncommon => RandomType::JokerUncommon,
            Rarity::Rare => RandomType::JokerRare,
            Rarity::Legendary => RandomType::JokerLegendary,
        };
        inst.rand_choice_common(kind, source, ante, pool)
    };

    let mut stickers = Stickers::default();
    if matches!(source, RandomSource::Shop | RandomSource::Buffoon) {
        let stake = inst.stake;
        let kind_sticker = if matches!(source, RandomSource::Buffoon) {
            RandomType::EternalPerishablePack
        } else { RandomType::EternalPerishable };
        let key = NodeKey::with_ante(kind_sticker, RandomSource::Null, ante);
        let poll = inst.random(key);
        if stake >= Stake::Black && poll > 0.7 && !ETERNAL_BLACKLIST.contains(&joker) {
            stickers.eternal = true;
        }
        if stake >= Stake::Orange && poll > 0.4 && poll <= 0.7 && !PERISHABLE_BLACKLIST.contains(&joker) {
            stickers.perishable = true;
        }
        if stake >= Stake::Gold {
            let rkind = if matches!(source, RandomSource::Buffoon) {
                RandomType::RentalPack
            } else { RandomType::Rental };
            let rkey = NodeKey::with_ante(rkind, RandomSource::Null, ante);
            stickers.rental = inst.random(rkey) > 0.7;
        }
    }
    let edition = next_joker_edition(inst, source, ante);
    (joker, rarity, edition, stickers)
}

/// `next_tarot` — lock-aware tarot draw with Soul check.
pub fn next_tarot(
    inst: &mut Instance, source: RandomSource, ante: i32, soulable: bool,
) -> &'static str {
    if soulable && !inst.is_locked("The Soul") {
        let key = NodeKey { kind: RandomType::Soul, source: Some(source), ante: Some(ante), resample: None };
        if inst.random(key) > 0.997 { return "The Soul"; }
    }
    inst.rand_choice_common(RandomType::Tarot, source, ante, TAROTS)
}

/// `next_planet` — lock-aware planet draw with Black Hole check.
pub fn next_planet(
    inst: &mut Instance, source: RandomSource, ante: i32, soulable: bool,
) -> &'static str {
    if soulable && !inst.is_locked("Black Hole") {
        let key = NodeKey { kind: RandomType::Soul, source: Some(source), ante: Some(ante), resample: None };
        if inst.random(key) > 0.997 { return "Black Hole"; }
    }
    inst.rand_choice_common(RandomType::Planet, source, ante, PLANETS)
}

/// `next_spectral` — lock-aware spectral with Soul / Black Hole.
pub fn next_spectral(
    inst: &mut Instance, source: RandomSource, ante: i32, soulable: bool,
) -> &'static str {
    if soulable {
        let mut forced: Option<&'static str> = None;
        if !inst.is_locked("The Soul") {
            let key = NodeKey { kind: RandomType::Soul, source: Some(source), ante: Some(ante), resample: None };
            if inst.random(key) > 0.997 { forced = Some("The Soul"); }
        }
        if !inst.is_locked("Black Hole") {
            let key = NodeKey { kind: RandomType::Soul, source: Some(source), ante: Some(ante), resample: None };
            if inst.random(key) > 0.997 { forced = Some("Black Hole"); }
        }
        if let Some(item) = forced { return item; }
    }
    inst.rand_choice_common(RandomType::Spectral, source, ante, SPECTRALS)
}

/// Weighted pack table: (name, weight). Total weight 22.42 per Immolate.
pub const PACK_WEIGHTS: &[(&str, f64)] = &[
    ("Arcana Pack", 4.0), ("Jumbo Arcana Pack", 2.0), ("Mega Arcana Pack", 0.5),
    ("Celestial Pack", 4.0), ("Jumbo Celestial Pack", 2.0), ("Mega Celestial Pack", 0.5),
    ("Standard Pack", 4.0), ("Jumbo Standard Pack", 2.0), ("Mega Standard Pack", 0.5),
    ("Buffoon Pack", 1.2), ("Jumbo Buffoon Pack", 0.6), ("Mega Buffoon Pack", 0.15),
    ("Spectral Pack", 0.6), ("Jumbo Spectral Pack", 0.3), ("Mega Spectral Pack", 0.07),
];
const PACKS_TOTAL_WEIGHT: f64 = 22.42;

/// `next_pack` — weighted pack draw.
pub fn next_pack(inst: &mut Instance, ante: i32) -> &'static str {
    let key = NodeKey { kind: RandomType::ShopPack, source: None, ante: Some(ante), resample: None };
    inst.rand_weighted_choice(key, PACK_WEIGHTS, PACKS_TOTAL_WEIGHT)
}

/// Contents drawn from a single booster pack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PackContents {
    Tarots(Vec<&'static str>),
    Planets(Vec<&'static str>),
    Spectrals(Vec<&'static str>),
    Jokers(Vec<&'static str>),
    Standard,
    Unknown,
}

/// Open one pack and return its contents. Mirrors
/// `{arcana,celestial,spectral,buffoon}_pack`, including the
/// temporary-lock loop that prevents intra-pack duplicates.
pub fn open_pack(inst: &mut Instance, pack: &str, ante: i32) -> PackContents {
    match pack {
        "Arcana Pack"        => open_arcana(inst, 3, ante),
        "Jumbo Arcana Pack"  => open_arcana(inst, 5, ante),
        "Mega Arcana Pack"   => open_arcana(inst, 5, ante),
        "Celestial Pack"       => open_celestial(inst, 3, ante),
        "Jumbo Celestial Pack" => open_celestial(inst, 5, ante),
        "Mega Celestial Pack"  => open_celestial(inst, 5, ante),
        "Spectral Pack"        => open_spectral(inst, 2, ante),
        "Jumbo Spectral Pack"  => open_spectral(inst, 4, ante),
        "Mega Spectral Pack"   => open_spectral(inst, 4, ante),
        "Buffoon Pack"        => open_buffoon(inst, 2, ante),
        "Jumbo Buffoon Pack"  => open_buffoon(inst, 4, ante),
        "Mega Buffoon Pack"   => open_buffoon(inst, 4, ante),
        "Standard Pack" | "Jumbo Standard Pack" | "Mega Standard Pack" => PackContents::Standard,
        _ => PackContents::Unknown,
    }
}

fn open_arcana(inst: &mut Instance, size: usize, ante: i32) -> PackContents {
    let mut out: Vec<&'static str> = Vec::with_capacity(size);
    for _ in 0..size {
        let t = next_tarot(inst, RandomSource::Arcana, ante, true);
        if !inst.showman { inst.lock(t); }
        out.push(t);
    }
    for t in &out { inst.unlock(*t); }
    PackContents::Tarots(out)
}
fn open_celestial(inst: &mut Instance, size: usize, ante: i32) -> PackContents {
    let mut out: Vec<&'static str> = Vec::with_capacity(size);
    for _ in 0..size {
        let p = next_planet(inst, RandomSource::Celestial, ante, true);
        if !inst.showman { inst.lock(p); }
        out.push(p);
    }
    for p in &out { inst.unlock(*p); }
    PackContents::Planets(out)
}
fn open_spectral(inst: &mut Instance, size: usize, ante: i32) -> PackContents {
    let mut out: Vec<&'static str> = Vec::with_capacity(size);
    for _ in 0..size {
        let s = next_spectral(inst, RandomSource::Spectral, ante, true);
        if !inst.showman { inst.lock(s); }
        out.push(s);
    }
    for s in &out { inst.unlock(*s); }
    PackContents::Spectrals(out)
}
fn open_buffoon(inst: &mut Instance, size: usize, ante: i32) -> PackContents {
    let mut out: Vec<&'static str> = Vec::with_capacity(size);
    for _ in 0..size {
        let j = next_joker(inst, RandomSource::Buffoon, ante);
        if !inst.showman { inst.lock(j); }
        out.push(j);
    }
    for j in &out { inst.unlock(*j); }
    PackContents::Jokers(out)
}

/// Mirrors `functions.cl::next_tag`.
pub fn next_tag(inst: &mut Instance, ante: i32) -> &'static str {
    let key = NodeKey::with_ante(RandomType::Tags, RandomSource::Null, ante);
    inst.rand_choice(key, TAGS)
}

/// Mirrors `functions.cl::next_voucher` (ignores lock state for now —
/// resample chain lands with voucher-chain tracking).
pub fn next_voucher(inst: &mut Instance, ante: i32) -> &'static str {
    let key = NodeKey {
        kind: RandomType::Voucher,
        source: None,
        ante: Some(ante),
        resample: None,
    };
    inst.rand_choice(key, VOUCHERS)
}

/// Mirrors `functions.cl::next_boss`. Finisher bosses appear on ante % 8 == 0,
/// small/big bosses otherwise. Applies ante-gated locks so e.g. The Plant
/// can't appear before ante 4.
pub fn next_boss(inst: &mut Instance, ante: i32) -> &'static str {
    inst.apply_ante_locks(ante);
    let pool: &[&'static str] = if ante % 8 == 0 { BOSSES_FINISHER } else { BOSSES_SMALL_BIG };
    inst.rand_choice_common(RandomType::Boss, RandomSource::Null, ante, pool)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_joker_draw() {
        let mut a = Instance::new("PHRAYJUS");
        let mut b = Instance::new("PHRAYJUS");
        for ante in 1..=4 {
            assert_eq!(
                next_joker(&mut a, RandomSource::Shop, ante),
                next_joker(&mut b, RandomSource::Shop, ante),
            );
        }
    }

    #[test]
    fn different_seeds_different_jokers() {
        // Statistical: across 100 seeds, ante-1 shop joker should not all match.
        let mut differs = false;
        let mut base = Instance::new("AAAA");
        let baseline = next_joker(&mut base, RandomSource::Shop, 1);
        for s in ["BBBB", "CCCC", "DDDD", "EEEE", "FFFF"] {
            let mut inst = Instance::new(s);
            if next_joker(&mut inst, RandomSource::Shop, 1) != baseline {
                differs = true;
                break;
            }
        }
        assert!(differs, "expected variety across seeds");
    }

    #[test]
    fn boss_pool_respects_ante() {
        let mut inst = Instance::new("BOSSEED1");
        let small_big = next_boss(&mut inst, 1);
        assert!(BOSSES_SMALL_BIG.contains(&small_big), "ante 1 boss not in small/big pool: {small_big}");

        let mut inst2 = Instance::new("BOSSEED1");
        let finisher = next_boss(&mut inst2, 8);
        assert!(BOSSES_FINISHER.contains(&finisher), "ante 8 boss not in finisher pool: {finisher}");
    }

    #[test]
    fn ante1_boss_excludes_locked_bosses() {
        let locked = ["The Plant", "The Tooth", "The Eye", "The Serpent",
                       "The Ox", "The Mouth", "The Fish", "The Wall",
                       "The House", "The Mark", "The Wheel", "The Arm",
                       "The Water", "The Needle", "The Flint"];
        for s in ["AAAA","BBBB","CCCC","DDDD","EEEE","PHRAYJUS","7LB2WVPK",
                   "BOSSEED1","SEEDABCD","ZZZZZ"] {
            let mut inst = Instance::new(s);
            let b = next_boss(&mut inst, 1);
            assert!(!locked.contains(&b),
                "ante 1 boss returned locked boss {b} for seed {s}");
        }
    }

    #[test]
    fn pack_weighted_draw_returns_valid_pack() {
        let known: Vec<&str> = PACK_WEIGHTS.iter().map(|(n, _)| *n).collect();
        for s in ["AAAA","BBBB","CCCC","DDDD","EEEE","PHRAYJUS","7LB2WVPK",
                   "BOSSEED1","SEEDABCD","ZZZZZ"] {
            let mut inst = Instance::new(s);
            let p = next_pack(&mut inst, 1);
            assert!(known.contains(&p), "unknown pack: {p}");
        }
    }

    #[test]
    fn pack_distribution_roughly_matches_weights() {
        let mut counts = std::collections::HashMap::<&'static str, usize>::new();
        let mut s = crate::Seed::from_rank(0, 6);
        for _ in 0..500 {
            let mut buf = [0u8; 8];
            let n = s.write_to(&mut buf);
            let seed_str = unsafe { core::str::from_utf8_unchecked(&buf[..n]) };
            let mut inst = Instance::new(seed_str);
            let p = next_pack(&mut inst, 1);
            *counts.entry(p).or_insert(0) += 1;
            s.increment();
        }
        assert!(*counts.get("Arcana Pack").unwrap_or(&0) >= 30, "Arcana too rare: {counts:?}");
        assert!(*counts.get("Celestial Pack").unwrap_or(&0) >= 30, "Celestial too rare");
        assert!(*counts.get("Mega Spectral Pack").unwrap_or(&0) < 15, "Mega Spectral too common");
    }

    #[test]
    fn arcana_pack_has_no_intra_pack_duplicates() {
        for s in ["AAAA","BBBB","CCCC","DDDD","EEEE","PHRAYJUS"] {
            let mut inst = Instance::new(s);
            let contents = open_pack(&mut inst, "Jumbo Arcana Pack", 2);
            if let PackContents::Tarots(items) = contents {
                let unique: std::collections::HashSet<_> = items.iter().collect();
                assert_eq!(unique.len(), items.len(),
                    "duplicate tarot in Jumbo Arcana for seed {s}: {items:?}");
            } else { panic!("expected Tarots variant"); }
        }
    }

    #[test]
    fn buffoon_pack_returns_jokers() {
        let mut inst = Instance::new("PHRAYJUS");
        let contents = open_pack(&mut inst, "Buffoon Pack", 2);
        match contents {
            PackContents::Jokers(items) => {
                assert_eq!(items.len(), 2);
                for j in &items {
                    let valid =
                        COMMON_JOKERS.contains(j) || UNCOMMON_JOKERS.contains(j) ||
                        RARE_JOKERS.contains(j) || LEGENDARY_JOKERS.contains(j);
                    assert!(valid, "unknown joker in buffoon pack: {j}");
                }
            }
            _ => panic!("expected Jokers variant"),
        }
    }

    #[test]
    fn stickers_off_on_white_stake() {
        let mut inst = Instance::new("PHRAYJUS");
        inst.stake = Stake::White;
        let (_j, _r, _e, s) = next_joker_with_stickers(&mut inst, RandomSource::Shop, 1);
        assert!(!s.eternal && !s.perishable && !s.rental);
    }

    #[test]
    fn stickers_eligible_on_black_stake() {
        let mut got_eternal = false;
        for s in ["AAAA","BBBB","CCCC","DDDD","EEEE","PHRAYJUS","7LB2WVPK",
                   "BOSSEED1","SEEDABCD","ZZZZZ","GGGGGG","HHHHHH"] {
            let mut inst = Instance::new(s);
            inst.stake = Stake::Black;
            let (_j, _r, _e, sticker) = next_joker_with_stickers(&mut inst, RandomSource::Shop, 1);
            if sticker.eternal { got_eternal = true; break; }
        }
        assert!(got_eternal, "expected at least one eternal sticker in 12 seeds at Black stake");
    }
}
