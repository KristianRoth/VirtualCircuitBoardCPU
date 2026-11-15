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
    let mut labels = false;
    let mut lines = text.lines().enumerate().map(|(i, l)| (i + 1, l));

    for (n, line) in lines.by_ref() {
        if line.trim().is_empty() {
            break;
        }
        let (key, value) = line.split_once('=').ok_or(format!("line {n}: expected `key = value`"))?;
        let (key, value) = (key.trim(), value.trim());
        // VMem preload: word.<address> = <value>, decimal or 0x hex.
        if let Some(address) = key.strip_prefix("word.") {
            let parse = |text: &str| match text.strip_prefix("0x") {
                Some(hex) => u32::from_str_radix(hex, 16).map_err(|_| format!("line {n}: \"{text}\" is not a number")),
                None => number::<u32>(text, n),
            };
            let address = parse(address)? as usize;
            *board.vmem.get_mut(address).ok_or(format!("line {n}: address {address} is outside VMem"))? = parse(value)?;
            continue;
        }
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
            "labels" => labels = value == "on",
            // VMem pins: bits, col, row, offsetX, offsetY (col,row relative to the art). Bit k is at (col - offsetX*k, row + offsetY*k).
            "vmem.address" | "vmem.content" => {
                let v: Vec<i64> = value.split(',').map(|t| number(t, n)).collect::<Result<_>>()?;
                let [bits, col, row, offset_x, offset_y] = v[..] else { return Err(format!("line {n}: expected bits,col,row,offsetX,offsetY")) };
                let at = if key == "vmem.address" { 0 } else { 7 };
                board.settings.vmem_settings[at..at + 7].copy_from_slice(&[bits, col + ORIGIN.0 as i64, row + ORIGIN.1 as i64, offset_x, offset_y, 1, 1]);
                board.settings.vmem_enabled = true;
            }
            // Persistent Memory From,To: the game copies this address range into the project when the simulation ends.
            "persistent" => {
                let (from, to) = value.split_once(',').ok_or(format!("line {n}: expected from,to"))?;
                board.settings.vmem_settings[14] = number(from, n)?;
                board.settings.vmem_settings[15] = number(to, n)?;
            }
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
    if labels {
        for (i, p) in probes.iter().enumerate() {
            draw_label(&mut board, i + 1, p)?;
        }
    }
    Ok(TextBoard { board, probes, ticks, covers })
}

/// Draws the probe's number in Annotation ink next to its LED block, so it can be read in the game:
/// below the block, or above it when there is no room below. Even numbers go one row of digits further
/// out, because neighbouring two-digit labels would overlap.
fn draw_label(board: &mut Board, number: usize, p: &Probe) -> Result<()> {
    let same = |board: &Board, y: usize| y < board.height && board.ink(p.x, y) == board.ink(p.x, p.y);
    let (mut top, mut bottom) = (p.y, p.y);
    while top > 0 && same(board, top - 1) {
        top -= 1;
    }
    while same(board, bottom + 1) {
        bottom += 1;
    }
    let text = number.to_string();
    let stagger = if number % 2 == 0 { 6 } else { 0 };
    let left = (p.x + 1).saturating_sub(text.len() * 2);
    let pixels = |first_row: usize| -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for (n, ch) in text.bytes().enumerate() {
            for (row, bits) in crate::render::DIGITS[(ch - b'0') as usize].iter().enumerate() {
                out.extend((0..3).filter(|col| bits & (4 >> col) != 0).map(|col| (left + n * 4 + col, first_row + row)));
            }
        }
        out
    };
    let free = |board: &Board, at: &[(usize, usize)]| at.iter().all(|(x, y)| *x < board.width && *y < board.height && board.ink(*x, *y) == Ink::Empty);
    let below = pixels(bottom + 2 + stagger);
    let above = (top >= 6 + stagger + ORIGIN.1).then(|| pixels(top - 6 - stagger));
    let place = if free(board, &below) { below } else { above.filter(|at| free(board, at)).ok_or(format!("probe \"{}\": no room for its label below or above", p.label))? };
    for (x, y) in place {
        board.set(x, y, Ink::Annotation);
    }
    Ok(())
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

        let tb = parse("vmem.address = 8,10,2,6,0\nvmem.content = 32,20,5,4,0\npersistent = 0,255\n\nL").unwrap();
        assert!(tb.board.settings.vmem_enabled);
        assert_eq!(tb.board.settings.vmem_settings, [8, 26, 18, 6, 0, 1, 1, 32, 36, 21, 4, 0, 1, 1, 0, 255]);

        let tb = parse("word.1 = 0xFFFFFFF2\nword.3 = 7\n\nL").unwrap();
        assert_eq!(tb.board.vmem[..4], [0, 0xFFFF_FFF2, 0, 7]);
    }

    #[test]
    fn mistakes_are_located() {
        let error = |text| parse(text).err().unwrap();
        assert!(error("\nLW\n.~").contains("line 3, column 2: unknown character '~'"));
        assert!(error("speed = 1\n\nL").contains("line 1: unknown key \"speed\""));
        assert!(error("probe.a = 0,0\nprobe.a = 0,0\n\nL").contains("used twice"));
        assert!(error("probe.a = 1,0\n\nL.").contains("probe \"a\": no ink at 1,0"));
    }

    #[test]
    fn labels_are_drawn_in_annotation_ink_below_the_probe() {
        let tb = parse("labels = on\nprobe.a = 1,0\n\nEEE\nEEE").unwrap();
        // "1" in the 3x5 font is .#. / ##. / .#. / .#. / ###, two rows below the block, centred on the probe.
        let row = |y: usize| (16..19).map(|x| tb.board.ink(x, 16 + y) == Ink::Annotation).collect::<Vec<_>>();
        assert_eq!(row(2), [false; 3]);
        assert_eq!(row(3), [false, true, false]);
        assert_eq!(row(4), [true, true, false]);
        assert_eq!(row(7), [true, true, true]);
        assert!(parse("labels = on\nprobe.a = 1,0\n\nEEE\n\n\n\nLLL").is_err(), "label would overwrite art");
        // With no room below, the label goes above the block (the art starts 8 rows down here).
        let tb = parse("labels = on\nprobe.a = 1,8\n\n.\n.\n.\n.\n.\n.\n.\n.\nEEE\nLLL").unwrap();
        assert_eq!(tb.board.ink(16, 16 + 6), Ink::Annotation); // bottom row of the "1": ###
        assert_eq!(tb.board.ink(17, 16 + 2), Ink::Annotation); // top row: .#.
    }
}
