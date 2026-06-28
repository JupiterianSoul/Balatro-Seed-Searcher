use balatro_seed_engine::derive::{next_joker, next_boss, next_tag, next_voucher};
use balatro_seed_engine::instance::{Instance, RandomSource};

fn main() {
    for seed in ["7LB2WZYX", "PHRAYJUS", "BLUEPRINT", "AAAAAAAA"] {
        println!("=== seed {seed} ===");
        for ante in 1..=4 {
            let mut i = Instance::new(seed);
            println!("  ante {ante}: joker={}, boss={}, tag={}, voucher={}",
                next_joker(&mut i, RandomSource::Shop, ante),
                next_boss(&mut i, ante),
                next_tag(&mut i, ante),
                next_voucher(&mut i, ante));
        }
    }
}
