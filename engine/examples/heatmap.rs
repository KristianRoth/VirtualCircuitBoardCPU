//! A heat map of how often each entity of a board flips during a run. Run from engine/:
//!   cargo run --release --example heatmap -- [ticks] [board.vcb] [program.vcbasm] [output prefix]
//! Writes <prefix>-all.png, <prefix>-simulated.png and <prefix>.md.

use std::fmt::Write;
use std::path::Path;
use vcb::netlist::{Components, Kind, NONE};
use vcb::render::{Image, DIGITS};
use vcb::sim::{Engine, Simulation};
use vcb::Board;

/// Flips per tick at the cold end of the ramp; anything rarer (but not zero) gets the same colour.
const LOW: f64 = 1e-6;
const BACKGROUND: [u8; 3] = [8, 8, 10];
const NEVER: [u8; 3] = [30, 40, 70];
const DROPPED: [u8; 3] = [72, 72, 72];
/// The ramp, evenly spaced on the logarithmic scale from LOW to 1 flip per tick.
const RAMP: [[u8; 3]; 5] = [[30, 60, 200], [0, 220, 220], [255, 230, 0], [255, 30, 0], [255, 255, 255]];
const CELL: usize = 64;

/// The colour of a position 0..=1 on the ramp.
fn ramp(t: f64) -> [u8; 3] {
    let p = t.clamp(0.0, 1.0) * (RAMP.len() - 1) as f64;
    let i = (p as usize).min(RAMP.len() - 2);
    std::array::from_fn(|c| (RAMP[i][c] as f64 + (RAMP[i + 1][c] as f64 - RAMP[i][c] as f64) * (p - i as f64)).round() as u8)
}

fn colour(flips_per_tick: f64) -> [u8; 3] {
    if flips_per_tick == 0.0 {
        return NEVER;
    }
    ramp(1.0 - flips_per_tick.log10() / LOW.log10())
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let arg = |i: usize, default: &str| args.get(i).cloned().unwrap_or(default.to_string());
    let count: u64 = arg(1, "3000000").parse().map_err(|e| format!("ticks: {e}"))?;
    let (board_path, program_path) = (arg(2, "../AoC2025/cpu-32bit-autorun-aoc1p1.vcb"), arg(3, "../AoC2025/days/day4/part2.vcbasm"));
    let prefix = arg(4, "../saves/pending/cpu-heatmap");

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
    let t = ticks as f64;

    // Per entity: pixel count, first pixel (top-left-most) and bounding box. Per grid cell: flips, shared out by pixels.
    let mut pixels = vec![0u64; n];
    let mut first = vec![(0usize, 0usize); n];
    let mut boxes = vec![(usize::MAX, usize::MAX, 0usize, 0usize); n];
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
    let mut zoom = (usize::MAX, usize::MAX, 0, 0); // the box of the simulated entities
    for (i, &e) in c.entity_of.iter().enumerate() {
        let (x, y) = (i % c.width, i / c.width);
        if e == NONE {
            continue;
        }
        (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1));
        let e = e as usize;
        if simulated[e] {
            zoom = (zoom.0.min(x), zoom.1.min(y), zoom.2.max(x + 1), zoom.3.max(y + 1));
        }
        if pixels[e] == 0 {
            first[e] = (x, y);
        }
        pixels[e] += 1;
        let b = &mut boxes[e];
        *b = (b.0.min(x), b.1.min(y), b.2.max(x), b.3.max(y));
    }
    if x1 == 0 {
        return Err("the board has no entities".into());
    }
    let cells_wide = c.width.div_ceil(CELL);
    let mut cells = vec![0f64; cells_wide * c.height.div_ceil(CELL)];
    for (i, &e) in c.entity_of.iter().enumerate() {
        if e != NONE && simulated[e as usize] {
            cells[(i / c.width / CELL) * cells_wide + i % c.width / CELL] += flips[e as usize] as f64 / pixels[e as usize] as f64;
        }
    }

    let total: u64 = flips.iter().sum();
    let total_simulated: u64 = (0..n).filter(|e| simulated[*e]).map(|e| flips[e]).sum();
    // An entity without pixels (none is expected) would make the cells miss its flips.
    assert!((cells.iter().sum::<f64>() - total_simulated as f64).abs() < 1.0, "the grid cells do not add up to the simulated flips");

    // The pictures: the entity area plus a margin, each board pixel `scale` image pixels, a legend strip below.
    // Each picture twice: the whole drawn board, and (-zoom) the area of the simulated entities with their surroundings.
    const LEGEND: usize = 90;
    let mut crops = String::new();
    let zoom = if zoom.2 == 0 { (x0, y0, x1, y1) } else { zoom };
    for (name, only_simulated, (x0, y0, x1, y1), margin) in [("all", false, (x0, y0, x1, y1), 4), ("simulated", true, (x0, y0, x1, y1), 4), ("all-zoom", false, zoom, 48), ("simulated-zoom", true, zoom, 48)] {
        let (x0, y0, x1, y1) = (x0.saturating_sub(margin), y0.saturating_sub(margin), (x1 + margin).min(c.width), (y1 + margin).min(c.height));
        let scale = (4500 / (x1 - x0)).max(1);
        let (width, board_height) = ((x1 - x0) * scale, (y1 - y0) * scale);
        let mut image = Image { width, height: board_height + LEGEND, rgba: vec![0xff; width * (board_height + LEGEND) * 4] };
        let fill = |image: &mut Image, x: usize, y: usize, w: usize, h: usize, rgb: [u8; 3]| {
            for py in y..(y + h).min(image.height) {
                for px in x..(x + w).min(image.width) {
                    image.rgba[(py * width + px) * 4..][..3].copy_from_slice(&rgb);
                }
            }
        };
        fill(&mut image, 0, 0, width, board_height + LEGEND, BACKGROUND);
        for y in y0..y1 {
            for x in x0..x1 {
                let e = c.entity_of[y * c.width + x];
                if e != NONE {
                    let rgb = if only_simulated && !simulated[e as usize] { DROPPED } else { colour(flips[e as usize] as f64 / t) };
                    fill(&mut image, (x - x0) * scale, (y - y0) * scale, scale, scale, rgb);
                }
            }
        }
        // Legend: swatches for "never" (and "not simulated"), then the ramp with a tick per decade.
        // The digit k under a tick means 10^-k flips per tick (0 = every tick, 6 = once in a million ticks).
        let top = board_height + 15;
        fill(&mut image, 20, top, 40, 30, NEVER);
        if only_simulated {
            fill(&mut image, 70, top, 40, 30, DROPPED);
        }
        let (bar_x, bar_w) = (140, 1200.min(width.saturating_sub(180)));
        for px in 0..bar_w {
            fill(&mut image, bar_x + px, top, 1, 30, ramp(px as f64 / (bar_w - 1) as f64));
        }
        let decades = -LOW.log10().round() as usize;
        for k in 0..=decades {
            let tx = bar_x + (bar_w - 1) * (decades - k) / decades;
            fill(&mut image, tx, top + 30, 2, 8, [255, 255, 255]);
            for (row, bits) in DIGITS[k].iter().enumerate() {
                for col in 0..3 {
                    if bits & (4 >> col) != 0 {
                        fill(&mut image, (tx + col * 5).saturating_sub(7), top + 42 + row * 5, 5, 5, [255, 255, 255]);
                    }
                }
            }
        }
        let path = format!("{prefix}-{name}.png");
        std::fs::write(&path, image.to_png()).map_err(|e| format!("{path}: {e}"))?;
        println!("wrote {path} ({width}x{} pixels)", board_height + LEGEND);
        writeln!(crops, "| `{}-{name}.png` | {x0}–{} | {y0}–{} | {scale}×{scale} |", Path::new(&prefix).file_name().unwrap().to_string_lossy(), x1 - 1, y1 - 1).unwrap();
    }

    // The summary.
    let per = |k: u64| k as f64 / t;
    let share = |k: f64, of: u64| 100.0 * k / (of.max(1)) as f64;
    let count_of = |want: bool| (0..n).filter(|e| simulated[*e] == want).count();
    let mut md = String::new();
    let w = &mut md;
    writeln!(w, "# Signal activity on the CPU\n").unwrap();
    writeln!(w, "Board `{board_path}`, program `{program_path}`, the first {ticks} ticks{}.\n", if engine.breakpoint_hit() { " (the run reached its Breakpoint)" } else { "" }).unwrap();
    writeln!(w, "A flip is one entity (a trace, a component such as a gate or latch, or a memory pin) going on or off in a tick.\n").unwrap();
    writeln!(w, "- The board has {n} entities. In an average tick {:.2} of them flip ({total} flips in all).", per(total)).unwrap();
    writeln!(w, "- {} entities never flipped in this run.", flips.iter().filter(|f| **f == 0).count()).unwrap();
    writeln!(w, "- The fast engine simulates {} entities; they make {:.1}% of all flips ({:.2} per tick).", count_of(true), share(total_simulated as f64, total), per(total_simulated)).unwrap();
    writeln!(w, "- It leaves out {} entities (ones whose state nothing depends on); they make the other {:.1}% ({:.2} per tick).\n", count_of(false), share((total - total_simulated) as f64, total), per(total - total_simulated)).unwrap();

    writeln!(w, "## The pictures\n").unwrap();
    writeln!(w, "`{0}-all.png` colours every entity; `{0}-simulated.png` draws the entities the fast engine leaves out in flat grey.", Path::new(&prefix).file_name().unwrap().to_string_lossy()).unwrap();
    writeln!(w, "The `-zoom` pictures show only the area around the simulated entities, larger. A board pixel (x, y) is at image pixel ((x − first x) × scale, (y − first y) × scale).\n").unwrap();
    writeln!(w, "| Picture | Board x | Board y | Image pixels per board pixel |\n|---|---|---|---|\n{crops}").unwrap();
    writeln!(w, "Colour scale (flips per tick, logarithmic, colours blend smoothly between the steps):\n").unwrap();
    writeln!(w, "| Flips per tick | Meaning | Colour (R, G, B) |\n|---|---|---|").unwrap();
    writeln!(w, "| no entity | empty, annotation, filler | near black {BACKGROUND:?} |").unwrap();
    writeln!(w, "| 0 | never flipped | dark grey-blue {NEVER:?} |").unwrap();
    writeln!(w, "| — | left out by the fast engine (second picture only) | grey {DROPPED:?} |").unwrap();
    for (i, name) in ["blue", "cyan", "yellow", "red", "white"].iter().enumerate() {
        let f = LOW.powf(1.0 - i as f64 / (RAMP.len() - 1) as f64);
        writeln!(w, "| {f:.1e}{} | about once in {:.0} ticks | {name} {:?} |", if i == 0 { " or less" } else { "" }, 1.0 / f, RAMP[i]).unwrap();
    }
    writeln!(w, "\nThe strip under the board is the legend: a swatch for \"never\" (and one for \"left out\" in the second picture), then the colour bar. The digit k under a tick mark means 10^−k flips per tick: 0 is every tick, 6 is once in a million ticks.\n").unwrap();

    writeln!(w, "## The 25 busiest simulated entities\n").unwrap();
    writeln!(w, "The pixel is the entity's top-most pixel (the left-most one of that row); the box is the area the whole entity covers.\n").unwrap();
    writeln!(w, "| # | Kind | Pixel (x, y) | Box x | Box y | Pixels | Flips per tick | Share of simulated flips |\n|---|---|---|---|---|---|---|---|").unwrap();
    let mut order: Vec<usize> = (0..n).filter(|e| simulated[*e]).collect();
    order.sort_by_key(|e| std::cmp::Reverse(flips[*e]));
    for (rank, &e) in order.iter().take(25).enumerate() {
        let kind = match c.kinds[e] {
            Kind::Trace => format!("trace ({})", board.inks[first[e].1 * c.width + first[e].0]),
            Kind::Component(ink) => format!("{ink}"),
            Kind::Pin => "memory pin".to_string(),
        };
        let b = boxes[e];
        writeln!(w, "| {} | {kind} | ({}, {}) | {}–{} | {}–{} | {} | {:.4} | {:.2}% |", rank + 1, first[e].0, first[e].1, b.0, b.2, b.1, b.3, pixels[e], per(flips[e]), share(flips[e] as f64, total_simulated)).unwrap();
    }

    writeln!(w, "\n## The 15 busiest regions\n").unwrap();
    writeln!(w, "The board is cut into {CELL}×{CELL}-pixel squares. An entity that spans several squares gives each a part of its flips in proportion to its pixels there. Only simulated entities count.\n").unwrap();
    writeln!(w, "| # | x | y | Flips per tick | Share of simulated flips |\n|---|---|---|---|---|").unwrap();
    let mut cell_order: Vec<usize> = (0..cells.len()).collect();
    cell_order.sort_by(|a, b| cells[*b].total_cmp(&cells[*a]));
    for (rank, &i) in cell_order.iter().take(15).enumerate() {
        let (cx, cy) = (i % cells_wide * CELL, i / cells_wide * CELL);
        writeln!(w, "| {} | {cx}–{} | {cy}–{} | {:.3} | {:.1}% |", rank + 1, cx + CELL - 1, cy + CELL - 1, cells[i] / t, share(cells[i], total_simulated)).unwrap();
    }
    let path = format!("{prefix}.md");
    std::fs::write(&path, &md).map_err(|e| format!("{path}: {e}"))?;
    println!("wrote {path}\n\n{md}");
    Ok(())
}
