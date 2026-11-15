//! Text-art boards: header lines `key = value`, a blank line, then one character per pixel.

use crate::board::Board;
use crate::format::Result;
use crate::ink::Ink;

/// Where the art's top-left corner goes on the 2048x2048 board.
pub const ORIGIN: (usize, usize) = (16, 16);
const BOARD_SIZE: usize = 2048;

/// Character → ink. Traces are the hex digits 0–f, buses u–z, wireless g–j.
pub const LEGEND: [(char, Ink); 49] = [
    ('.', Ink::Empty),
    ('0', Ink::Trace(0)), ('1', Ink::Trace(1)), ('2', Ink::Trace(2)), ('3', Ink::Trace(3)),
    ('4', Ink::Trace(4)), ('5', Ink::Trace(5)), ('6', Ink::Trace(6)), ('7', Ink::Trace(7)),
    ('8', Ink::Trace(8)), ('9', Ink::Trace(9)), ('a', Ink::Trace(10)), ('b', Ink::Trace(11)),
    ('c', Ink::Trace(12)), ('d', Ink::Trace(13)), ('e', Ink::Trace(14)), ('f', Ink::Trace(15)),
    ('R', Ink::Read), ('W', Ink::Write), ('+', Ink::Cross), ('T', Ink::Tunnel), ('M', Ink::Mesh),
    ('u', Ink::Bus(0)), ('v', Ink::Bus(1)), ('w', Ink::Bus(2)), ('x', Ink::Bus(3)), ('y', Ink::Bus(4)), ('z', Ink::Bus(5)),
    ('B', Ink::Buffer), ('A', Ink::And), ('O', Ink::Or), ('X', Ink::Xor),
    ('N', Ink::Not), ('D', Ink::Nand), ('Q', Ink::Nor), ('Z', Ink::Xnor),
    ('L', Ink::LatchOn), ('l', Ink::LatchOff), ('C', Ink::Clock), ('I', Ink::Timer), ('?', Ink::Random),
    ('E', Ink::Led), ('!', Ink::Breakpoint),
    ('g', Ink::Wireless(0)), ('h', Ink::Wireless(1)), ('i', Ink::Wireless(2)), ('j', Ink::Wireless(3)),
    ('#', Ink::Annotation), ('%', Ink::Filler),
];

pub struct Probe {
    pub label: String,
    /// Board coordinates.
    pub x: usize,
    pub y: usize,
}

pub struct TextBoard {
    pub board: Board,
    pub probes: Vec<Probe>,
    /// The ticks the checklist asks about.
    pub ticks: Vec<u64>,
    /// Rule and question ids this board is meant to cover.
    pub covers: Vec<String>,
}

fn number<T: std::str::FromStr>(text: &str, line: usize) -> Result<T> {
    text.trim().parse().map_err(|_| format!("line {line}: \"{}\" is not a number", text.trim()))
}

pub fn parse(text: &str) -> Result<TextBoard> {
    let mut board = Board::new(BOARD_SIZE, BOARD_SIZE);
    let (mut probes, mut ticks, mut covers): (Vec<Probe>, Vec<u64>, Vec<String>) = Default::default();
    let mut lines = text.lines().enumerate().map(|(i, l)| (i + 1, l));

    for (n, line) in lines.by_ref() {
        if line.trim().is_empty() {
            break;
        }
        let (key, value) = line.split_once('=').ok_or(format!("line {n}: expected `key = value`"))?;
        let (key, value) = (key.trim(), value.trim());
        if let Some(label) = key.strip_prefix("probe.") {
            let (col, row) = value.split_once(',').ok_or(format!("line {n}: expected `col,row`"))?;
            if probes.iter().any(|p| p.label == label) {
                return Err(format!("line {n}: probe label \"{label}\" is used twice"));
            }
            probes.push(Probe { label: label.into(), x: ORIGIN.0 + number::<usize>(col, n)?, y: ORIGIN.1 + number::<usize>(row, n)? });
            continue;
        }
        match key {
            "clock_interval" => board.settings.clock_interval = number(value, n)?,
            "random_seed" => board.settings.random_seed = number(value, n)?,
            "ticks" => match value.split_once("..") {
                Some((a, b)) => ticks = (number(a, n)?..=number(b, n)?).collect(),
                None => ticks = value.split(',').map(|t| number(t, n)).collect::<Result<_>>()?,
            },
            "covers" => covers = value.split(',').map(|c| c.trim().to_string()).collect(),
            _ => return Err(format!("line {n}: unknown key \"{key}\"")),
        }
    }

    let (mut art_width, mut art_height) = (0, 0);
    for (row, (n, line)) in lines.enumerate() {
        for (col, ch) in line.chars().enumerate() {
            if ch == ' ' {
                continue;
            }
            let ink = LEGEND.iter().find(|(c, _)| *c == ch).ok_or(format!("line {n}, column {}: unknown character '{ch}'", col + 1))?.1;
            if col + ORIGIN.0 >= BOARD_SIZE || row + ORIGIN.1 >= BOARD_SIZE {
                return Err(format!("line {n}, column {}: outside the {BOARD_SIZE}x{BOARD_SIZE} board", col + 1));
            }
            board.set(ORIGIN.0 + col, ORIGIN.1 + row, ink);
            if ink != Ink::Empty {
                art_width = art_width.max(col + 1);
                art_height = row + 1;
            }
        }
    }
    for p in &probes {
        if p.x >= ORIGIN.0 + art_width || p.y >= ORIGIN.1 + art_height || board.ink(p.x, p.y) == Ink::Empty {
            return Err(format!("probe \"{}\": no ink at {},{} (relative to the art)", p.label, p.x - ORIGIN.0, p.y - ORIGIN.1));
        }
    }
    Ok(TextBoard { board, probes, ticks, covers })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legend_has_one_character_for_every_ink() {
        for (_, ink) in crate::ink::COLOURS {
            assert_eq!(LEGEND.iter().filter(|(_, i)| *i == ink).count(), 1, "{ink}");
        }
        let mut chars: Vec<char> = LEGEND.iter().map(|(c, _)| *c).collect();
        chars.sort();
        chars.dedup();
        assert_eq!(chars.len(), LEGEND.len(), "a character is used twice");
    }

    #[test]
    fn art_is_placed_at_the_origin_with_settings_and_probes() {
        let tb = parse("clock_interval = 3\nticks = 0..2\ncovers = rule1, 6\nprobe.led = 1,1\n\nL W\n.E").unwrap();
        assert_eq!(tb.board.ink(16, 16), Ink::LatchOn);
        assert_eq!(tb.board.ink(17, 16), Ink::Empty);
        assert_eq!(tb.board.ink(18, 16), Ink::Write);
        assert_eq!(tb.board.ink(17, 17), Ink::Led);
        assert_eq!(tb.board.settings.clock_interval, 3);
        assert!(!tb.board.settings.random_is_time_seed);
        assert_eq!(tb.ticks, [0, 1, 2]);
        assert_eq!(tb.covers, ["rule1", "6"]);
        assert_eq!((tb.probes[0].label.as_str(), tb.probes[0].x, tb.probes[0].y), ("led", 17, 17));
    }

    #[test]
    fn mistakes_are_located() {
        let error = |text| parse(text).err().unwrap();
        assert!(error("\nLW\n.~").contains("line 3, column 2: unknown character '~'"));
        assert!(error("speed = 1\n\nL").contains("line 1: unknown key \"speed\""));
        assert!(error("probe.a = 0,0\nprobe.a = 0,0\n\nL").contains("used twice"));
        assert!(error("probe.a = 1,0\n\nL.").contains("probe \"a\": no ink at 1,0"));
    }
}
