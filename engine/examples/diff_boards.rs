//! Prints the pixels that differ between two boards, as text art around the changed area.
//! Run from engine/: cargo run --example diff_boards -- a.vcb b.vcb

use std::path::Path;
use vcb::{textboard::LEGEND, Board};

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let (a, b) = (Board::read(Path::new(&args[1]))?, Board::read(Path::new(&args[2]))?);
    println!("settings a: {:?}\nsettings b: {:?}", a.settings, b.settings);
    let diff: Vec<usize> = (0..a.inks.len()).filter(|i| a.inks[*i] != b.inks[*i]).collect();
    println!("{} pixels differ", diff.len());
    let Some(&first) = diff.first() else { return Ok(()) };
    let (mut x0, mut x1, mut y0, mut y1) = (first % a.width, first % a.width, first / a.width, first / a.width);
    for i in &diff {
        (x0, x1, y0, y1) = (x0.min(i % a.width), x1.max(i % a.width), y0.min(i / a.width), y1.max(i / a.width));
    }
    let ch = |ink| LEGEND.iter().find(|(_, i)| *i == ink).unwrap().0;
    for (name, board) in [("a", &a), ("b", &b)] {
        println!("{name}: x {}..={}, y {}..={}", x0.saturating_sub(2), x1 + 2, y0.saturating_sub(2), y1 + 2);
        for y in y0.saturating_sub(2)..=y1 + 2 {
            println!("{}", (x0.saturating_sub(2)..=x1 + 2).map(|x| ch(board.ink(x, y))).collect::<String>());
        }
    }
    Ok(())
}
