//! Item tables — jokers, tarots, planets, spectrals, vouchers, tags, bosses.
//!
//! These match Balatro 1.0.x's declared order, which is the order Immolate /
//! TheSoul use for their hash tables. Editions follow the documented weight
//! tables (1280 base, modifiers from vouchers / Glow Up tag / etc.).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum Joker {
    Joker, GreedyJoker, LustyJoker, WrathfulJoker, GluttonousJoker, JollyJoker,
    ZanyJoker, MadJoker, CrazyJoker, DrollJoker, SlyJoker, WilyJoker,
    CleverJoker, DeviousJoker, CraftyJoker, HalfJoker, JokerStencil,
    FourFingers, Mime, CreditCard, CeremonialDagger, Banner, MysticSummit,
    MarbleJoker, LoyaltyCard, EightBall, Misprint, Dusk, RaisedFist,
    ChaosTheClown, Fibonacci, SteelJoker, ScaryFace, AbstractJoker, DelayedGratification,
    HackJoker, PareidoliaJoker, GrosMichel, EvenSteven, OddTodd, Scholar,
    BusinessCard, Supernova, RideTheBus, SpaceJoker, Egg, Burglar, Blackboard,
    Runner, IceCreamJoker, DNA, Splash, BlueJoker, SixthSense, Constellation,
    Hiker, FacelessJoker, GreenJoker, Superposition, ToDoList, Cavendish,
    CardSharp, RedCard, Madness, SquareJoker, Seance, RiffRaff, Vampire,
    Shortcut, HologramJoker, Vagabond, Baron, CloudNine, Rocket, Obelisk,
    MidasMask, Luchador, Photograph, GiftCard, TurtleBean, Erosion, ReservedParking,
    MailInRebate, ToTheMoon, Hallucination, FortuneTeller, Juggler, Drunkard,
    StoneJoker, GoldenJoker, LuckyCat, BaseballCard, Bull, DietCola, TradingCard,
    FlashCard, Popcorn, SpareTrousers, AncientJoker, Ramen, WalkieTalkie,
    SeltzerJoker, Castle, SmileyFace, CampfireJoker, GoldenTicket, MrBones,
    Acrobat, SockAndBuskin, Swashbuckler, Troubadour, Certificate, SmearedJoker,
    Throwback, HangingChad, RoughGem, Bloodstone, Arrowhead, OnyxAgate, GlassJoker,
    ShowmanJoker, FlowerPot, Blueprint, WeeJoker, MerryAndy, OopsAllSixes, IdolJoker,
    SeeingDouble, Matador, HitTheRoad, TheDuo, TheTrio, TheFamily, TheOrder,
    TheTribe, StuntmanJoker, InvisibleJoker, BrainstormJoker, SatelliteJoker,
    ShootTheMoon, DriversLicense, CartomancerJoker, AstronomerJoker, BurntJoker,
    Bootstraps, CanioJoker, TriboulteJoker, YorickJoker, ChicotJoker, PerketJoker,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edition { Base, Foil, Holographic, Polychrome, Negative }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rarity { Common, Uncommon, Rare, Legendary }

/// Edition draw — base weights from Balatro 1.0.x.
/// Modifiers (Glow Up tag, Hone voucher, etc.) get applied on top by the
/// caller. Returns (Edition, used roll for closeness scoring).
#[inline]
pub fn roll_edition(roll: f64) -> Edition {
    // Game weights: Negative 0.3%, Polychrome 0.3%, Holographic 1.4%, Foil 2.0%, Base 96%.
    // Cumulative from highest priority: Negative checked first.
    if roll > 0.997 { Edition::Negative }
    else if roll > 0.994 { Edition::Polychrome }
    else if roll > 0.980 { Edition::Holographic }
    else if roll > 0.960 { Edition::Foil }
    else { Edition::Base }
}

/// Rarity draw for a joker shop slot.
#[inline]
pub fn roll_rarity(roll: f64) -> Rarity {
    // 70% common, 25% uncommon, 5% rare. Legendary is only via Soul card.
    if roll > 0.95 { Rarity::Rare }
    else if roll > 0.70 { Rarity::Uncommon }
    else { Rarity::Common }
}
