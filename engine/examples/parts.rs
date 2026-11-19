//! Sparsely connected parts of a board, as a hierarchy, drawn onto the board. Run from engine/:
//!   cargo run --release --example parts -- [ticks] [board.vcb] [program.vcbasm] [output prefix] [T,T,T]
//! Writes <prefix>-level1.png, -level2.png, ... (one per threshold T, highest first) and <prefix>.md.
//!
//! The graph: the simulated entities; a component or pin is connected to every trace it reads or writes.
//! For a threshold T every trace with more than T connections is cut; what still hangs together is a part.

use std::cmp::Reverse;
use std::fmt::Write;
use std::path::Path;
use vcb::netlist::{Components, Kind, NONE};
use vcb::render::Image;
use vcb::sim::{Engine, Simulation};
use vcb::{Board, Ink};

const BACKGROUND: [u8; 3] = [8, 8, 10];
const DROPPED: [u8; 3] = [52, 52, 52];
const FRAGMENT: [u8; 3] = [125, 110, 95];
const CUT: [u8; 3] = [255, 255, 255];
/// A part with fewer entities than this is a fragment.
const SMALL: usize = 4;
/// "Few inputs" in the summary means fewer than this.
const FEW_INPUTS: usize = 10;
/// The thresholds drawn when none are given: picked by hand from the T table of the default run (see the summary).
const DEFAULT_LEVELS: &str = "16,14,7,3";
/// Written by hand on 2026-11-19 after looking at the pictures of the default run. Printed only for that run.
const OBSERVATIONS: &str = r#"## What stands out

Why these four thresholds were drawn:

- **Level 1, T = 16.** Down to T = 19 almost nothing happens: one part keeps 92% of the flips. Between T = 18 and T = 16 the first whole blocks come loose (43 parts at 16), but the main part still holds 82% of the flips.
- **Level 2, T = 14.** The largest change in the table: the share of flips in the largest part falls from 82% to 19%, because the clock divider becomes a part of its own.
- **Level 3, T = 7.** The largest part falls from 1274 to 757 entities and the number of parts goes from 97 to 129.
- **Level 4, T = 3.** The finest level that still has parts: the largest part has 436 entities. At T = 2 the cut traces carry 24% of the flips and there are 963 fragments, so that level is not drawn.

Are the parts recognisable blocks? Partly. What the pictures show (positions are board coordinates):

- **The clock divider is found exactly.** From level 2 on, a part of 22 entities with 8 latches at x 1091–1109, y 249–257 has no inputs and one output, and makes 57.7% of all simulated flips. It holds 18 of the 23 entities of the self-running clock part; one more is the cut trace that leaves it, and the last 4 are, from level 3 on, in a 7-entity part beside it (x 1116–1120, y 261–266, 2.3% of the flips).
- **Three 16-bit blocks in the middle column (x 988–1067) come out whole** at levels 1 and 2: y 264–298 (272 entities, 17 latches, 20 inputs, 16 outputs, 6.1% of the flips), y 331–360 (256 entities, 17 latches, 3 inputs, 16 outputs, 1.4%) and, at level 1 only, y 218–229 (145 entities, no latches). The first two stay whole down to level 4 (257 and 256 entities there).
- **The other register rows fall apart into one part per bit**, not one part per register: parts of 7 or 9 entities with 3 inputs and 1 output, side by side (x 990–1068 at y 218–229, 242–253 and 313–320, and from level 3 the two long rows at about x 768–928, y 222–262). The method cannot join the bits of a row, because what joins them is the row's widely read control trace, and that is what gets cut.
- **A small block at x 1179–1191, y 256–272** (45 entities, 1 input, no outputs, 4.0% of the flips) is separate from level 2 on.
- **The large grid at the right (about x 1116–1260, y 275–365) and the left-hand rows stay one part** through levels 1 to 3 (2184, 2096, then 757 entities), together with the small groups at the top (about x 769–837, y 162–195 and x 1003–1081, y 161–196). At level 4 the grid breaks into overlapping row and column parts that do not look like blocks. The four rows at about x 792–924, y 358–428 are only fragments from level 3 on.
- **The block at x 797–925, y 310–347** splits at level 3 into two interleaved parts of 207 and 208 entities (41 and 42 inputs), and at level 4 mostly into fragments.
- **The white column in the middle (about x 933–980, y 186–435)** is cut at every level: these are the widely read lines between the left and the right half. So are the dotted lines at the top (about x 910–1003, y 139–185).

Do parts with few inputs hold most of the activity? From T = 14 down, yes, but mostly because of one part:

- At T = 16, 40 of 43 parts have fewer than 10 inputs and hold 4.6% of the flips.
- At T = 14, 60 of 62 parts have fewer than 10 inputs and hold 71.3%. Of that, 57.7 points are the clock divider alone (no inputs); the other 59 hold 13.6%. One part of 2096 entities with 56 inputs holds 19.1%.
- At T = 7 it is 125 of 129 parts and 75.4%; at T = 3, 166 of 172 parts and 78.3%, with 4.6% in fragments and 6.4% on cut traces.

The ten most widely connected traces are in the table above: five have 37 connections and all ten have one writer and 32 to 36 readers. All ten together flip less than 0.03 times per tick: the big connectors are control lines that rarely change, not busy data lines.
"#;

/// The parts for one threshold. `part[e]` is NONE for a cut trace and for an entity that is not simulated.
struct Level {
    t: usize,
    part: Vec<u32>,
    members: Vec<Vec<usize>>,
    /// Per part: the number of cut traces its components read / write.
    inputs: Vec<usize>,
    outputs: Vec<usize>,
    flips: Vec<u64>,
    cut: Vec<usize>,
}

fn find(parent: &mut [u32], mut i: u32) -> u32 {
    while parent[i as usize] != i {
        parent[i as usize] = parent[parent[i as usize] as usize];
        i = parent[i as usize];
    }
    i
}

fn level(c: &Components, simulated: &[bool], degree: &[usize], flips: &[u64], t: usize) -> Level {
    let n = c.kinds.len();
    let kept = |e: usize| simulated[e] && degree[e] <= t; // a component's degree is 0 here, so it is always kept
    let components = || (0..n).filter(|e| simulated[*e] && c.kinds[*e] != Kind::Trace);
    let mut parent: Vec<u32> = (0..n as u32).collect();
    for e in components() {
        for &trace in c.reads[e].iter().chain(&c.writes[e]).filter(|x| kept(**x as usize)) {
            let (a, b) = (find(&mut parent, e as u32), find(&mut parent, trace));
            parent[a.max(b) as usize] = a.min(b); // the lowest id stays the root
        }
    }
    let (mut part, mut members, mut cut) = (vec![NONE; n], Vec::<Vec<usize>>::new(), Vec::new());
    for e in (0..n).filter(|e| simulated[*e]) {
        if !kept(e) {
            cut.push(e);
            continue;
        }
        let root = find(&mut parent, e as u32) as usize; // root <= e, so it was numbered first
        if part[root] == NONE {
            part[root] = members.len() as u32;
            members.push(Vec::new());
        }
        part[e] = part[root];
        members[part[e] as usize].push(e);
    }
    let mut ends = [vec![Vec::<u32>::new(); members.len()], vec![Vec::new(); members.len()]];
    for e in components() {
        for (side, list) in [&c.reads[e], &c.writes[e]].into_iter().enumerate() {
            ends[side][part[e] as usize].extend(list.iter().filter(|x| simulated[**x as usize] && !kept(**x as usize)));
        }
    }
    let [inputs, outputs] = ends.map(|side| side.into_iter().map(|mut v| { v.sort(); v.dedup(); v.len() }).collect());
    let flips = members.iter().map(|m| m.iter().map(|e| flips[*e]).sum()).collect();
    Level { t, part, members, inputs, outputs, flips, cut }
}

fn hsv(h: f64, s: f64, v: f64) -> [u8; 3] {
    std::array::from_fn(|i| {
        let k = (h * 6.0 + [5.0, 3.0, 1.0][i]) % 6.0;
        ((v - v * s * k.min(4.0 - k).clamp(0.0, 1.0)) * 255.0).round() as u8
    })
}

/// Everything reachable forwards (trace → readers, component → written traces) from the entities `start` picks.
fn downstream(c: &Components, start: impl Fn(Kind) -> bool) -> Vec<bool> {
    let mut seen = vec![false; c.kinds.len()];
    let mut todo: Vec<usize> = (0..c.kinds.len()).filter(|e| start(c.kinds[*e])).collect();
    while let Some(e) = todo.pop() {
        if !std::mem::replace(&mut seen[e], true) {
            todo.extend((if c.kinds[e] == Kind::Trace { &c.readers[e] } else { &c.writes[e] }).iter().map(|x| *x as usize));
        }
    }
    seen
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let arg = |i: usize, default: &str| args.get(i).cloned().unwrap_or(default.to_string());
    let count: u64 = arg(1, "3000000").parse().map_err(|e| format!("ticks: {e}"))?;
    let (board_path, program_path) = (arg(2, "../AoC2025/cpu-32bit-autorun-aoc1p1.vcb"), arg(3, "../AoC2025/days/day4/part2.vcbasm"));
    let prefix = arg(4, "../saves/pending/cpu-parts");
    let wanted: Vec<usize> = arg(5, DEFAULT_LEVELS).split(',').filter_map(|t| t.trim().parse().ok()).collect();
    let default_run = args.len() <= 1;
    let short = Path::new(&prefix).file_name().unwrap().to_string_lossy().to_string();

    let mut board = Board::read(Path::new(&board_path))?;
    let source = std::fs::read_to_string(&program_path).map_err(|e| e.to_string())?.replace("\r\n", "\n");
    vcb::asm::assemble(&source, &program_path, &mut board, false)?;
    let c = Components::build(&board)?;
    let n = c.kinds.len();

    // What the fast engine simulates. A trace with one writer is merged into that writer there, so it counts with it.
    let fast = vcb::sim2::Engine::new(&board, c.clone(), &Default::default())?;
    let simulated: Vec<bool> = (0..n).map(|e| fast.live()[e] || (c.kinds[e] == Kind::Trace && c.writers[e].len() == 1 && fast.live()[c.writers[e][0] as usize])).collect();

    let mut engine = Engine::new(&board, c.clone(), &Default::default())?;
    let mut previous = engine.state().to_vec();
    let mut flips = vec![0u64; n];
    let mut ticks = 0u64;
    while ticks < count && !engine.breakpoint_hit() {
        engine.step();
        ticks += 1;
        let state = engine.state();
        for e in 0..n {
            flips[e] += (state[e] != previous[e]) as u64;
        }
        previous.copy_from_slice(state);
    }
    let per = |k: u64| k as f64 / ticks as f64;
    let total: u64 = (0..n).filter(|e| simulated[*e]).map(|e| flips[e]).sum();
    let share = |k: u64| 100.0 * k as f64 / total.max(1) as f64;

    // Per entity: first pixel (top-left-most) and bounding box. The crop: the box of the simulated entities.
    let mut first = vec![None; n];
    let mut boxes = vec![(usize::MAX, usize::MAX, 0usize, 0usize); n];
    let mut zoom = (usize::MAX, usize::MAX, 0, 0);
    for (i, &e) in c.entity_of.iter().enumerate().filter(|(_, e)| **e != NONE) {
        let (x, y, e) = (i % c.width, i / c.width, e as usize);
        if simulated[e] {
            zoom = (zoom.0.min(x), zoom.1.min(y), zoom.2.max(x + 1), zoom.3.max(y + 1));
        }
        first[e].get_or_insert((x, y));
        let b = &mut boxes[e];
        *b = (b.0.min(x), b.1.min(y), b.2.max(x), b.3.max(y));
    }
    if zoom.2 == 0 {
        return Err("the board has no simulated entities".into());
    }
    let pixel = |e: usize| first[e].map_or("no pixel".to_string(), |(x, y)| format!("({x}, {y})"));
    const MARGIN: usize = 48;
    let (x0, y0, x1, y1) = (zoom.0.saturating_sub(MARGIN), zoom.1.saturating_sub(MARGIN), (zoom.2 + MARGIN).min(c.width), (zoom.3 + MARGIN).min(c.height));
    let scale = (4500 / (x1 - x0)).max(1);

    // The degree of a trace: the simulated components that read it plus those that write it.
    let degree: Vec<usize> = (0..n).map(|e| c.readers[e].iter().chain(&c.writers[e]).filter(|x| simulated[**x as usize]).count()).collect();
    let max_degree = (0..n).filter(|e| simulated[*e]).map(|e| degree[e]).max().unwrap_or(0);
    let mut has_degree = vec![false; max_degree + 2];
    (0..n).filter(|e| simulated[*e] && c.kinds[*e] == Kind::Trace).for_each(|e| has_degree[degree[e]] = true);

    // The self-running clock part, as examples/autonomous.rs finds it: downstream of a clock, never of a pin, Random or Timer.
    let tainted = downstream(&c, |k| matches!(k, Kind::Pin | Kind::Component(Ink::Random | Ink::Timer)));
    let driven = downstream(&c, |k| k == Kind::Component(Ink::Clock));
    let auto: Vec<usize> = (0..n).filter(|e| simulated[*e] && driven[*e] && !tainted[*e]).collect();

    // Every threshold from the highest degree down to 2. A T that no trace has as its degree plus one gives the
    // partition of the row above, so it is not computed again.
    let mut table = String::new();
    let mut drawn: Vec<Level> = Vec::new();
    for t in (2..=max_degree).rev() {
        let changes = t == max_degree || has_degree[t + 1];
        if !changes && !wanted.contains(&t) {
            continue;
        }
        let l = level(&c, &simulated, &degree, &flips, t);
        let cut_flips: u64 = l.cut.iter().map(|e| flips[*e]).sum();
        assert_eq!(l.flips.iter().sum::<u64>() + cut_flips, total, "T = {t}: parts, fragments and cut traces do not add up to the simulated flips");
        let big: Vec<usize> = (0..l.members.len()).filter(|p| l.members[*p].len() >= SMALL).collect();
        let largest = (0..l.members.len()).max_by_key(|p| l.members[*p].len()).unwrap();
        let few: Vec<usize> = big.iter().copied().filter(|p| l.inputs[*p] < FEW_INPUTS).collect();
        if changes {
            writeln!(table, "| {t} | {} | {:.1}% | {} | {} | {} | {:.1}% | {} | {:.1}% |", l.cut.len(), share(cut_flips), big.len(), l.members.len() - big.len(), l.members[largest].len(), share(l.flips[largest]), few.len(), share(few.iter().map(|p| l.flips[*p]).sum())).unwrap();
        }
        if wanted.contains(&t) {
            drawn.push(l);
        }
    }
    // The tree: every part at a lower threshold lies inside one part at the next higher drawn threshold.
    for pair in drawn.windows(2) {
        for members in &pair[1].members {
            let above = pair[0].part[members[0]];
            assert!(above != NONE && members.iter().all(|e| pair[0].part[*e] == above), "T = {}: a part is not inside one part of T = {}", pair[1].t, pair[0].t);
        }
    }

    let mut md = String::new();
    let w = &mut md;
    writeln!(w, "# Parts of the CPU found by cutting the widely shared traces\n").unwrap();
    writeln!(w, "Board `{board_path}`, program `{program_path}`, the first {ticks} ticks{}.\n", if engine.breakpoint_hit() { " (the run reached its Breakpoint)" } else { "" }).unwrap();
    writeln!(w, "Only the {} entities the fast engine simulates are looked at ({} components and memory pins, {} traces). A component is connected to every trace it reads and every trace it writes. The degree of a trace is the number of simulated components that read it plus the number that write it. For a threshold T, every trace with a degree above T is cut out; what still hangs together through the remaining traces is a **part**. The cut traces are the connections between parts. Lowering T only cuts more traces, so a part at a lower T always lies inside one part at a higher T: the levels form a tree. A part with fewer than {SMALL} entities is called a fragment. An input of a part is a cut trace that one of its components reads; an output is a cut trace that one of its components writes. A flip is one entity going on or off in a tick; the run had {total} simulated flips ({:.2} per tick).\n", simulated.iter().filter(|s| **s).count(), (0..n).filter(|e| simulated[*e] && c.kinds[*e] != Kind::Trace).count(), (0..n).filter(|e| simulated[*e] && c.kinds[*e] == Kind::Trace).count(), per(total)).unwrap();

    writeln!(w, "## What each threshold gives\n").unwrap();
    writeln!(w, "Every T from the highest degree ({max_degree}) down to 2. A T that is missing gives exactly the parts of the row above it (no trace has the degree T + 1). \"Few inputs\" means fewer than {FEW_INPUTS}.\n").unwrap();
    writeln!(w, "| T | Cut traces | Flips on cut traces | Parts ({SMALL}+ entities) | Fragments | Entities in the largest part | Flips in the largest part | Parts with few inputs | Flips in parts with few inputs |\n|---|---|---|---|---|---|---|---|---|\n{table}").unwrap();

    writeln!(w, "## The pictures\n").unwrap();
    writeln!(w, "Each picture shows board x {x0}–{}, y {y0}–{}, {scale}×{scale} image pixels per board pixel: board pixel (x, y) is at image pixel ((x − {x0}) × {scale}, (y − {y0}) × {scale}). Every part has its own colour (a colour is used again for parts far from each other; the 20 parts in each table have colours no other part of that level uses). Cut traces are white `#FFFFFF`, fragments are brown-grey `#{:02X}{:02X}{:02X}`, entities the fast engine leaves out are dark grey, empty board is near black. Colours are chosen per level: a part does not keep its colour from one level to the next.\n", x1 - 1, y1 - 1, FRAGMENT[0], FRAGMENT[1], FRAGMENT[2]).unwrap();

    let palette: Vec<[u8; 3]> = [(1.0, 1.0), (0.55, 1.0), (1.0, 0.62)].iter().flat_map(|&(s, v)| (0..24).map(move |h| hsv(h as f64 / 24.0, s, v))).collect();
    for (index, l) in drawn.iter().enumerate() {
        let parts = l.members.len();
        let big = |p: usize| l.members[p].len() >= SMALL;
        // Labels: the parts that are not fragments, by flips, 1 first.
        let mut order: Vec<usize> = (0..parts).filter(|p| big(*p)).collect();
        order.sort_by_key(|p| Reverse(l.flips[*p]));
        let mut label = vec![0usize; parts];
        order.iter().enumerate().for_each(|(rank, p)| label[*p] = rank + 1);
        let bbox: Vec<_> = l.members.iter().map(|m| m.iter().fold((usize::MAX, usize::MAX, 0, 0), |b, e| (b.0.min(boxes[*e].0), b.1.min(boxes[*e].1), b.2.max(boxes[*e].2), b.3.max(boxes[*e].3)))).collect();

        // Colours. Two parts are neighbours when they have pixels in the same 8×8 square. In order of flips, a part
        // takes the palette colour furthest from its neighbours' colours, the least used one among equals.
        const CELL: usize = 8;
        let cells_wide = c.width.div_ceil(CELL);
        let mut cell_parts = vec![Vec::new(); cells_wide * c.height.div_ceil(CELL)];
        let mut part_cells = vec![Vec::new(); parts];
        for (i, &e) in c.entity_of.iter().enumerate().filter(|(_, e)| **e != NONE) {
            let p = l.part[e as usize] as usize;
            let cell = i / c.width / CELL * cells_wide + i % c.width / CELL;
            if p < parts && big(p) && cell_parts[cell].last() != Some(&p) && !cell_parts[cell].contains(&p) {
                cell_parts[cell].push(p);
                part_cells[p].push(cell);
            }
        }
        let mut colour: Vec<Option<[u8; 3]>> = vec![None; parts];
        let mut used = vec![0usize; palette.len()];
        let mut reserved = Vec::new(); // the colours of the 20 parts in the table
        for (rank, &p) in order.iter().enumerate() {
            // For the table's 20 parts the colours already given to them count as neighbours too, so those 20 differ clearly from each other.
            let near: Vec<[u8; 3]> = part_cells[p].iter().flat_map(|cell| &cell_parts[*cell]).filter_map(|q| colour[*q]).chain(reserved.iter().copied().filter(|_| rank < 20)).collect();
            let distance = |a: [u8; 3]| near.iter().map(|b| (0..3).map(|k| (a[k] as i32 - b[k] as i32).pow(2)).sum::<i32>()).min().unwrap_or(i32::MAX);
            let best = (0..palette.len()).filter(|i| !reserved.contains(&palette[*i])).max_by_key(|i| (distance(palette[*i]), Reverse(used[*i]))).unwrap();
            colour[p] = Some(palette[best]);
            used[best] += 1;
            if rank < 20 {
                reserved.push(palette[best]);
            }
        }
        let hex = |p: usize| colour[p].map_or("fragment".to_string(), |c| format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2]));

        let (width, height) = ((x1 - x0) * scale, (y1 - y0) * scale);
        let mut image = Image { width, height, rgba: vec![0xff; width * height * 4] };
        for y in y0..y1 {
            for x in x0..x1 {
                let rgb = match c.entity_of[y * c.width + x] {
                    NONE => BACKGROUND,
                    e if !simulated[e as usize] => DROPPED,
                    e => match l.part[e as usize] {
                        NONE => CUT,
                        p => colour[p as usize].unwrap_or(FRAGMENT),
                    },
                };
                for py in (y - y0) * scale..(y - y0 + 1) * scale {
                    for px in (x - x0) * scale..(x - x0 + 1) * scale {
                        image.rgba[(py * width + px) * 4..][..3].copy_from_slice(&rgb);
                    }
                }
            }
        }
        let path = format!("{prefix}-level{}.png", index + 1);
        std::fs::write(&path, image.to_png()).map_err(|e| format!("{path}: {e}"))?;
        println!("wrote {path} ({width}x{height} pixels)");

        let sum = |ps: &[usize]| ps.iter().map(|p| l.flips[*p]).sum::<u64>();
        let fragments: Vec<usize> = (0..parts).filter(|p| !big(*p)).collect();
        let few: Vec<usize> = order.iter().copied().filter(|p| l.inputs[*p] < FEW_INPUTS).collect();
        let cut_flips: u64 = l.cut.iter().map(|e| flips[*e]).sum();
        let latches = |p: usize| l.members[p].iter().filter(|e| matches!(c.kinds[**e], Kind::Component(Ink::LatchOn | Ink::LatchOff))).count();
        writeln!(w, "## Level {}: T = {} (`{short}-level{}.png`)\n", index + 1, l.t, index + 1).unwrap();
        writeln!(w, "- {} parts, the largest with {} entities; {} fragments with {} entities in all.", order.len(), l.members.iter().map(|m| m.len()).max().unwrap_or(0), fragments.len(), fragments.iter().map(|p| l.members[*p].len()).sum::<usize>()).unwrap();
        writeln!(w, "- {} cut traces (white) carry {:.1}% of the flips ({:.2} per tick).", l.cut.len(), share(cut_flips), per(cut_flips)).unwrap();
        writeln!(w, "- {} of the {} parts have fewer than {FEW_INPUTS} inputs; they hold {:.1}% of the flips. The {} parts with {FEW_INPUTS} or more inputs hold {:.1}%.", few.len(), order.len(), share(sum(&few)), order.len() - few.len(), share(sum(&order) - sum(&few))).unwrap();
        // Where the self-running clock part falls.
        let mut homes: Vec<(usize, usize)> = Vec::new(); // (part, entities of the clock part in it)
        for &e in auto.iter().filter(|e| l.part[**e] != NONE) {
            match homes.iter_mut().find(|h| h.0 == l.part[e] as usize) {
                Some(h) => h.1 += 1,
                None => homes.push((l.part[e] as usize, 1)),
            }
        }
        homes.sort_by_key(|h| Reverse(h.1));
        let in_parts: Vec<String> = homes.iter().filter(|h| big(h.0)).map(|&(p, k)| format!("{k} in part {} ({}, {} entities, x {}–{}, y {}–{})", label[p], hex(p), l.members[p].len(), bbox[p].0, bbox[p].2, bbox[p].1, bbox[p].3)).collect();
        writeln!(w, "- The self-running clock part (driven by a clock, never influenced by a memory pin) has {} simulated entities: {}{}; {} in fragments; {} are cut traces.\n", auto.len(), in_parts.iter().take(8).cloned().collect::<Vec<_>>().join(", "), if in_parts.len() > 8 { format!(" and {} more parts", in_parts.len() - 8) } else { String::new() }, homes.iter().filter(|h| !big(h.0)).map(|h| h.1).sum::<usize>(), auto.iter().filter(|e| l.part[**e] == NONE).count()).unwrap();

        writeln!(w, "The 20 parts with the most flips. The pixel is the top-left-most pixel of the part's first entity.\n").unwrap();
        writeln!(w, "| Part | Colour | Entities (components / traces) | Latches | Inputs | Outputs | Share of flips | Flips per tick | Box x | Box y | Pixel (x, y) |\n|---|---|---|---|---|---|---|---|---|---|---|").unwrap();
        for &p in order.iter().take(20) {
            let traces = l.members[p].iter().filter(|e| c.kinds[**e] == Kind::Trace).count();
            writeln!(w, "| {} | `{}` | {} ({} / {traces}) | {} | {} | {} | {:.2}% | {:.3} | {}–{} | {}–{} | {} |", label[p], hex(p), l.members[p].len(), l.members[p].len() - traces, latches(p), l.inputs[p], l.outputs[p], share(l.flips[p]), per(l.flips[p]), bbox[p].0, bbox[p].2, bbox[p].1, bbox[p].3, pixel(l.members[p][0])).unwrap();
        }
        let rest = &order[order.len().min(20)..];
        writeln!(w, "| the other {} parts | | {} | {} | | | {:.2}% | {:.3} | | | |", rest.len(), rest.iter().map(|p| l.members[*p].len()).sum::<usize>(), rest.iter().map(|p| latches(*p)).sum::<usize>(), share(sum(rest)), per(sum(rest))).unwrap();
        writeln!(w, "| all {} fragments | `#{:02X}{:02X}{:02X}` | {} | {} | | | {:.2}% | {:.3} | | | |", fragments.len(), FRAGMENT[0], FRAGMENT[1], FRAGMENT[2], fragments.iter().map(|p| l.members[*p].len()).sum::<usize>(), fragments.iter().map(|p| latches(*p)).sum::<usize>(), share(sum(&fragments)), per(sum(&fragments))).unwrap();
        writeln!(w, "| the {} cut traces | `#FFFFFF` | {} | | | | {:.2}% | {:.3} | | | |\n", l.cut.len(), l.cut.len(), share(cut_flips), per(cut_flips)).unwrap();
    }

    writeln!(w, "## The big connectors\n").unwrap();
    writeln!(w, "The ten traces with the highest degree. A trace is cut at every T below its degree.\n").unwrap();
    writeln!(w, "| Degree | Readers | Writers | Pixel (x, y) | Box x | Box y | Flips per tick |\n|---|---|---|---|---|---|---|").unwrap();
    let mut by_degree: Vec<usize> = (0..n).filter(|e| simulated[*e] && c.kinds[*e] == Kind::Trace).collect();
    by_degree.sort_by_key(|e| Reverse(degree[*e]));
    for &e in by_degree.iter().take(10) {
        let live = |list: &[u32]| list.iter().filter(|x| simulated[**x as usize]).count();
        writeln!(w, "| {} | {} | {} | {} | {}–{} | {}–{} | {:.4} |", degree[e], live(&c.readers[e]), live(&c.writers[e]), pixel(e), boxes[e].0, boxes[e].2, boxes[e].1, boxes[e].3, per(flips[e])).unwrap();
    }
    if default_run {
        writeln!(w, "\n{OBSERVATIONS}").unwrap();
    }
    writeln!(w, "\n## What the tool does not do\n").unwrap();
    writeln!(w, "- It knows nothing about what a part computes. Parts come from the wiring alone; the flips are only measured afterwards.").unwrap();
    writeln!(w, "- It cuts by one number, the degree of a trace. A trace with few connections that joins two large blocks is never cut, and a widely read trace inside one block is always cut. It does not search for the cut with the fewest connections.").unwrap();
    writeln!(w, "- It does not look at direction or at loops: it does not say whether a part's outputs can come back to its inputs within the part, or in which order parts would have to be evaluated.").unwrap();
    writeln!(w, "- Entities the fast engine leaves out are ignored, also as connections. A trace's readers that are left out do not count towards its degree.").unwrap();
    writeln!(w, "- The flips come from one run of one program. Another program can make other parts busy.").unwrap();
    writeln!(w, "- It does not measure whether treating parts as units would make the engine faster.").unwrap();
    let path = format!("{prefix}.md");
    std::fs::write(&path, &md).map_err(|e| format!("{path}: {e}"))?;
    println!("wrote {path}\n\n{md}");
    Ok(())
}
