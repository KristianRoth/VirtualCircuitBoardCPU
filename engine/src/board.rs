//! A parsed `.vcb` project.

use crate::ink::Ink;
use serde_json::{Map, Value};

/// VMem is always 2^20 big-endian 32-bit words.
pub const VMEM_WORDS: usize = 1 << 20;
pub const VMEM_BYTES: usize = VMEM_WORDS * 4;

#[derive(Clone, PartialEq, Debug)]
pub struct Settings {
    pub vmem_enabled: bool,
    /// [address bits, x, y, offsetX, offsetY, sizeX, sizeY, content bits, x, y, offsetX, offsetY, sizeX, sizeY, persistent from, persistent to]
    pub vmem_settings: [i64; 16],
    pub clock_interval: i64,
    /// Milliseconds in the game. Read, but not used by v1.
    pub timer_interval: i64,
    pub random_seed: i64,
    pub random_is_time_seed: bool,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Board {
    pub width: usize,
    pub height: usize,
    /// One ink per pixel, row by row.
    pub inks: Vec<Ink>,
    pub vmem: Vec<u32>,
    pub settings: Settings,
    /// Every other field of the file, kept so a re-written file still opens in the game.
    pub(crate) json: Map<String, Value>,
}

impl Board {
    pub fn ink(&self, x: usize, y: usize) -> Ink {
        self.inks[y * self.width + x]
    }

    pub fn set(&mut self, x: usize, y: usize, ink: Ink) {
        self.inks[y * self.width + x] = ink;
    }
}
