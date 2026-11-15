//! Tick-exact simulation engine for Virtual Circuit Board `.vcb` files.
//! Specs: ../specs/vcb-engine.md, vcb-engine-v1.md and vcb-game-facts.md.

pub mod board;
pub mod format;
pub mod ink;

pub use board::Board;
pub use ink::Ink;

use std::collections::BTreeMap;
use std::fmt::Write;

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
    out
}
