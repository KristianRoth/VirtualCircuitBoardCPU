//! Ink types and the colour→ink table for VCB Update 1.0.1.
//! The canonical copy of the table is in specs/vcb-game-facts.md.

use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum Ink {
    Empty,
    /// One of the 16 trace colours.
    Trace(u8),
    Read,
    Write,
    Cross,
    Tunnel,
    Mesh,
    /// One of the 6 bus colours.
    Bus(u8),
    Buffer,
    And,
    Or,
    Xor,
    Not,
    Nand,
    Nor,
    Xnor,
    LatchOn,
    LatchOff,
    Clock,
    Timer,
    Random,
    Led,
    Breakpoint,
    /// Wireless 0–3.
    Wireless(u8),
    Annotation,
    Filler,
}

use Ink::*;

/// RGB colour (alpha is always ff) → ink.
pub const COLOURS: [(u32, Ink); 48] = [
    (0x2a3541, Trace(0)),
    (0x9fa8ae, Trace(1)),
    (0xa1555e, Trace(2)),
    (0xa16c56, Trace(3)),
    (0xa18556, Trace(4)),
    (0xa19856, Trace(5)),
    (0x99a156, Trace(6)),
    (0x88a156, Trace(7)),
    (0x6ca156, Trace(8)),
    (0x56a18d, Trace(9)),
    (0x5693a1, Trace(10)),
    (0x567ba1, Trace(11)),
    (0x5662a1, Trace(12)),
    (0x6656a1, Trace(13)),
    (0x8756a1, Trace(14)),
    (0xa15597, Trace(15)),
    (0x2e475d, Read),
    (0x4d383e, Write),
    (0x66788e, Cross),
    (0x535572, Tunnel),
    (0x646a57, Mesh),
    (0x7a2f24, Bus(0)),
    (0x3e7a24, Bus(1)),
    (0x24417a, Bus(2)),
    (0x25627a, Bus(3)),
    (0x7a2d66, Bus(4)),
    (0x7a7024, Bus(5)),
    (0x92ff63, Buffer),
    (0xffc663, And),
    (0x63f2ff, Or),
    (0xae74ff, Xor),
    (0xff628a, Not),
    (0xffa200, Nand),
    (0x30d9ff, Nor),
    (0xa600ff, Xnor),
    (0x63ff9f, LatchOn),
    (0x384d47, LatchOff),
    (0xff0041, Clock),
    (0xff6700, Timer),
    (0xe5ff00, Random),
    (0xffffff, Led),
    (0xe00000, Breakpoint),
    (0xff00bf, Wireless(0)),
    (0xff00af, Wireless(1)),
    (0xff009f, Wireless(2)),
    (0xff008f, Wireless(3)),
    (0x3a4551, Annotation),
    (0x8caba1, Filler),
];

impl Ink {
    /// `None` for a colour that is not in the table.
    pub fn from_rgba(px: [u8; 4]) -> Option<Ink> {
        if px == [0, 0, 0, 0] {
            return Some(Empty);
        }
        if px[3] != 0xff {
            return None;
        }
        let rgb = u32::from_be_bytes([0, px[0], px[1], px[2]]);
        COLOURS.iter().find(|(c, _)| *c == rgb).map(|(_, ink)| *ink)
    }

    pub fn to_rgba(self) -> [u8; 4] {
        match COLOURS.iter().find(|(_, ink)| *ink == self) {
            Some((c, _)) => {
                let b = c.to_be_bytes();
                [b[1], b[2], b[3], 0xff]
            }
            None => [0, 0, 0, 0], // Empty
        }
    }
}

/// The game version every table and fixture in this crate is taken from.
pub const GAME_VERSION: &str = "1.0.1";

/// Units whose behaviour is proven by observed fixtures. A unit is added in the same commit as those fixtures.
pub const VERIFIED: &[&str] = &[
    // `gates` and `gates-recheck`, observed 2026-11-15.
    "Trace", "Read", "Write", "Buffer", "AND", "OR", "XOR", "NOT", "NAND", "NOR", "XNOR", "LED",
    // `routing-a`, `routing-b`, `general` and `general-1`, observed 2026-11-15.
    "Cross", "Tunnel", "Bus", "Mesh", "Wireless", "Clock", "Breakpoint", "Latch On", "Latch Off",
    // `random-vmem-seed-1` and `-2`, recorded 2026-11-15: the engine reproduces all 2 x 4064 bits (MT19937).
    "Random",
    // VMem: `vmem-preserve`, the random recorder saves and `vmem-timing` (ticks and save), 2026-11-16.
    "vmem",
];
/// Units that cannot match the game tick for tick and are only checked for what can be observed.
pub const MODELLED: &[&str] = &[
    // The game's timers follow the wall clock. `random-pulse` (2026-11-16) shows only that a timer starts off.
    "Timer",
];

impl Ink {
    /// The gate unit this ink belongs to: the ink kind without its colour. `None` for inks without logic.
    pub fn unit(self) -> Option<&'static str> {
        Some(match self {
            Empty | Annotation | Filler => return None,
            Trace(_) => "Trace",
            Bus(_) => "Bus",
            Wireless(_) => "Wireless",
            Read => "Read",
            Write => "Write",
            Cross => "Cross",
            Tunnel => "Tunnel",
            Mesh => "Mesh",
            Buffer => "Buffer",
            And => "AND",
            Or => "OR",
            Xor => "XOR",
            Not => "NOT",
            Nand => "NAND",
            Nor => "NOR",
            Xnor => "XNOR",
            LatchOn => "Latch On",
            LatchOff => "Latch Off",
            Clock => "Clock",
            Timer => "Timer",
            Random => "Random",
            Led => "LED",
            Breakpoint => "Breakpoint",
        })
    }
}

impl fmt::Display for Ink {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Trace(n) => write!(f, "Trace {n}"),
            Bus(n) => write!(f, "Bus {n}"),
            Wireless(n) => write!(f, "Wireless {n}"),
            LatchOn => write!(f, "Latch On"),
            LatchOff => write!(f, "Latch Off"),
            Led => write!(f, "LED"),
            And | Or | Xor | Not | Nand | Nor | Xnor => write!(f, "{}", format!("{self:?}").to_uppercase()),
            other => write!(f, "{other:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_colour_round_trips() {
        for (_, ink) in COLOURS {
            assert_eq!(Ink::from_rgba(ink.to_rgba()), Some(ink));
        }
        assert_eq!(Ink::from_rgba(Empty.to_rgba()), Some(Empty));
    }

    #[test]
    fn unknown_colours_are_rejected() {
        assert_eq!(Ink::from_rgba([1, 2, 3, 0xff]), None);
        assert_eq!(Ink::from_rgba([0x92, 0xff, 0x63, 0x80]), None); // Buffer with wrong alpha
    }
}
