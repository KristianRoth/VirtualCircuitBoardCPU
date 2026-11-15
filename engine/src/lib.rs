//! Tick-exact simulation engine for Virtual Circuit Board `.vcb` files.
//! Specs: ../specs/vcb-engine.md, vcb-engine-v1.md and vcb-game-facts.md.

pub mod board;
pub mod format;
pub mod ink;
pub mod netlist;
pub mod render;
pub mod sim;
pub mod textboard;

pub use board::Board;
pub use ink::Ink;

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt::Write;

/// The first pixel of every unit on the board that is neither verified nor modelled.
/// VMem counts as the unit "vmem" when it is enabled.
pub fn unverified_units(board: &Board) -> Vec<(&'static str, Option<(usize, usize)>)> {
    let known = |unit: &str| ink::VERIFIED.contains(&unit) || ink::MODELLED.contains(&unit);
    let mut found: Vec<(&'static str, Option<(usize, usize)>)> = Vec::new();
    for (i, ink) in board.inks.iter().enumerate() {
        if let Some(unit) = ink.unit() {
            if !known(unit) && !found.iter().any(|(u, _)| *u == unit) {
                found.push((unit, Some((i % board.width, i / board.width))));
            }
        }
    }
    if board.settings.vmem_enabled && !known("vmem") {
        found.push(("vmem", None));
    }
    found
}

/// The text printed by `vcb info`.
pub fn info(board: &Board) -> String {
    let s = &board.settings;
    let mut out = String::new();
    writeln!(out, "size: {}x{}", board.width, board.height).unwrap();
    writeln!(out, "clock_interval: {}", s.clock_interval).unwrap();
    writeln!(out, "timer_interval: {} ms", s.timer_interval).unwrap();
    writeln!(out, "random_seed: {} (time seed: {})", s.random_seed, s.random_is_time_seed).unwrap();
    writeln!(out, "vmem: {}", if s.vmem_enabled { "enabled" } else { "disabled" }).unwrap();
    writeln!(out, "vmem_settings: {:?}", s.vmem_settings).unwrap();
    writeln!(out, "vmem non-zero words: {}", board.vmem.iter().filter(|w| **w != 0).count()).unwrap();

    let mut counts: BTreeMap<Ink, usize> = BTreeMap::new();
    for ink in &board.inks {
        if *ink != Ink::Empty {
            *counts.entry(*ink).or_default() += 1;
        }
    }
    writeln!(out, "inks: {}", counts.len()).unwrap();
    for (ink, n) in counts {
        writeln!(out, "  {ink}: {n}").unwrap();
    }
    let unverified: Vec<&str> = unverified_units(board).into_iter().map(|(unit, _)| unit).collect();
    writeln!(out, "not verified yet: {}", if unverified.is_empty() { "nothing".into() } else { unverified.join(", ") }).unwrap();
    out
}

/// The blank checklist the user fills in while stepping the board in the game.
/// It never contains what the engine expects, so the observation stays independent.
pub fn checklist(name: &str, vcb_text: &str, tb: &textboard::TextBoard) -> String {
    let mut out = String::new();
    writeln!(out, "# Checklist: {name}\n").unwrap();
    writeln!(out, "- Board: `{name}.vcb` (picture with numbered LEDs: `{name}.png`)").unwrap();
    writeln!(out, "- Board SHA-256: `{:x}`", Sha256::digest(vcb_text.as_bytes())).unwrap();
    writeln!(out, "- Game version: {}", ink::GAME_VERSION).unwrap();
    writeln!(out, "- Covers: {}\n", tb.covers.join(", ")).unwrap();
    writeln!(out, "Open the board, start the simulation paused, and step one tick at a time.").unwrap();
    writeln!(out, "For each tick below, write the numbers of the LEDs that are lit (or `none`).\n").unwrap();
    writeln!(out, "## LEDs\n\n| # | Label | Board x,y |\n|---|---|---|").unwrap();
    for (i, p) in tb.probes.iter().enumerate() {
        writeln!(out, "| {} | {} | {},{} |", i + 1, p.label, p.x, p.y).unwrap();
    }
    writeln!(out, "\n## Observations\n").unwrap();
    for t in &tb.ticks {
        writeln!(out, "- tick {t}, lit LEDs: ").unwrap();
    }
    writeln!(out, "\n## Other\n\n- Tick counter when a Breakpoint paused the game (if any): ").unwrap();
    writeln!(out, "- Anything unexpected: ").unwrap();
    out
}
