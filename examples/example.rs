//! Tyche dice-rolling examples, escalating from basic to complex.
//!
//! Tyche uses FoundryVTT-style notation (not prose), e.g. `4d6kh2` means
//! "roll four six-sided dice and keep the highest two".
//!
//! Run with: cargo run --example example

use tyche::dice::roller::FastRand;
use tyche::{dice::Roller, Dice, Expr};

fn main() {
    let mut rng = FastRand::default();

    // 1. A single six-sided die
    // 40k relevance: initiative roll (simple check)
    println!("=== 1. Single d6 — initiative ===");
    example_single_die(&mut rng);

    // 2. Four six-sided dice, no modifiers
    // 40k relevance: weapon skill test (3d6+BS)
    println!("\n=== 2. Four d6s — raw rolls ===");
    example_four_d6(&mut rng);

    // 3. Dice with a flat bonus
    // 40k relevance: attack roll — 3d6 + Weapon Skill bonus
    println!("\n=== 3. Three d6 plus bonus ===");
    example_three_d6_plus_bonus(&mut rng);

    // 4. Keep highest — drop the worst results
    // 40k relevance: hit on 3+ with a marine's 3d6kh2 (BS30)
    println!("\n=== 4. Three d6 keep-highest-2 ===");
    example_keep_highest(&mut rng);

    // 5. Keep lowest — force consistency at the bottom
    // 40k relevance: Morale check where you want to see the spread
    println!("\n=== 5. Four d6 keep-lowest-2 ===");
    example_keep_lowest(&mut rng);

    // 6. Exploding dice — roll extra on max (recursive)
    // 40k relevance: shooting with heavy weapon, explode on unmodified hit of 6
    println!("\n=== 6. Four d6 exploding ===");
    example_explode(&mut rng);

    // 7. Reroll failed dice — once per die that meets a condition
    // 40k relevance: re-roll 1s to hit (Astartes instinctive reaction)
    println!("\n=== 7. Three d6 reroll ones ===");
    example_reroll(&mut rng);

    // 8. Combined expression with +, -, and parens
    // 40k relevance: combat — (3d6kh2+BS) vs defense, subtract armor save
    println!("\n=== 8. Compound formula ===");
    example_compound_formula(&mut rng);

    // 9. Programmatic Dice builder with modifiers
    // 40k relevance: a bolter profile built in code instead of typed as a string
    println!("\n=== 9. Programmatic bolter roll ===");
    example_programmatic(&mut rng);

    // 10. Everything together — recursive explode + reroll + compound math
    // 40k relevance: lascannon crit build with rerolls and multi-hit
    println!("\n=== 10. Full chaos — lascarannon barrage ===");
    example_full_chaos(&mut rng);
}

// ---------------------------------------------------------------------------
// Examples
// ---------------------------------------------------------------------------

fn example_single_die(rng: &mut FastRand) {
    let expr: Expr = "1d6".parse().unwrap();
    let rolled = expr.eval(rng);
    println!("1d6  =>  {rolled:#?}\n");
}

fn example_four_d6(rng: &mut FastRand) {
    let expr: Expr = "4d6".parse().unwrap();
    let rolled = expr.eval(rng);
    println!("4d6  =>  {rolled:#?}\n");
}

fn example_three_d6_plus_bonus(rng: &mut FastRand) {
    // Roll 3d6 and add +2 bonus
    let expr: Expr = "3d6 + 2".parse().unwrap();
    let rolled = expr.eval(rng);
    println!("3d6+2  =>  {rolled:#?}\n");
}

fn example_keep_highest(rng: &mut FastRand) {
    // Roll 4d6, keep the 2 highest — equivalent to k3 in 40k terminology
    let expr: Expr = "4d6kh2".parse().unwrap();
    let rolled = expr.eval(rng);
    println!("4d6kh2  =>  {rolled:#?}\n");
}

fn example_keep_lowest(rng: &mut FastRand) {
    // Roll 4d6, keep the 2 lowest — useful for stress/danger tables
    let expr: Expr = "4d6kl2".parse().unwrap();
    let rolled = expr.eval(rng);
    println!("4d6kl2  =>  {rolled:#?}\n");
}

fn example_explode(rng: &mut FastRand) {
    // Explode on max — rolling a 6 gives another d6 (recursive)
    let expr: Expr = "4d6xo".parse().unwrap();
    let rolled = expr.eval(rng);
    println!("4d6xo  =>  {rolled:#?}\n");
}

fn example_reroll(rng: &mut FastRand) {
    // Reroll dice showing less than 3 (recursive reroll of re-rolls)
    let expr: Expr = "4d6rr<3".parse().unwrap();
    let rolled = expr.eval(rng);
    println!("4d6rr<3  =>  {rolled:#?}\n");
}

fn example_compound_formula(rng: &mut FastRand) {
    // (3d6 keep-highest-2 + 5) minus (2d6 reroll ones)
    let expr: Expr = "(3d6kh2 + 5) - 2d6r<2".parse().unwrap();
    let rolled = expr.eval(rng);
    println!("(3d6kh2 + 5) - 2d6r<2  =>  {rolled:#?}\n");
}

fn example_programmatic(rng: &mut FastRand) {
    // Build a bolter attack in code: 4 dice, d6, reroll 1s, explode on 6s
    let bolter = Dice::builder()
        .count(4)
        .sides(6)
        .reroll(tyche::dice::modifier::Condition::Eq(1), false) // once only
        .build();

    let result = rng.roll(&bolter, true).unwrap();
    println!("Bolter (programmatic)  =>  {result:#?}\n");
}

fn example_full_chaos(rng: &mut FastRand) {
    // Lascannon volley: 3 lance shots, each explodes on a 6,
    // then add a separate wound check (2d6+1).
    let expr: Expr = "3d6xo + 2d6+1".parse().unwrap();
    let rolled = expr.eval(rng);
    println!("3d6xo + 2d6+1  =>  {rolled:#?}\n");
}
