//! Inspects a board's wiring and signals. Run from engine/:
//!   cargo run --release --example trace -- <board.vcb> art <x0> <y0> <x1> <y1>      the pixels as text art
//!   cargo run --release --example trace -- <board.vcb> at <x,y> [<x,y> ...]         what each entity connects to
//!   cargo run --release --example trace -- <board.vcb> bus <x,y>                    a bus and the traces touching it
//!   cargo run --release --example trace -- <board.vcb> watch <program.vcbasm> <from tick> <to tick> <x,y=name> ...
//!                                                                              every on/off change of those entities

use std::path::Path;
use vcb::netlist::{Components, Kind};
use vcb::sim::{Engine, Simulation};
use vcb::{textboard::LEGEND, Board};

fn xy(text: &str) -> (usize, usize) {
    let (x, y) = text.split_once(',').expect("x,y");
    (x.parse().unwrap(), y.parse().unwrap())
}

/// "Trace 5 #123 at 10,20 (17 px, x 10..30 y 20..25)"
fn describe(board: &Board, c: &Components, e: u32) -> String {
    let pixels: Vec<usize> = (0..c.entity_of.len()).filter(|i| c.entity_of[*i] == e).collect();
    let (xs, ys): (Vec<usize>, Vec<usize>) = pixels.iter().map(|i| (i % c.width, i / c.width)).unzip();
    let first = pixels[0];
    let what = match c.kinds[e as usize] {
        Kind::Trace => format!("{:?}", board.inks[first]),
        Kind::Component(ink) => format!("{ink:?}"),
        Kind::Pin => "VMem pin".to_string(),
    };
    format!(
        "{what} #{e} at {},{} ({} px, x {}..{} y {}..{})",
        first % c.width,
        first / c.width,
        pixels.len(),
        xs.iter().min().unwrap(),
        xs.iter().max().unwrap(),
        ys.iter().min().unwrap(),
        ys.iter().max().unwrap()
    )
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let mut board = Board::read(Path::new(&args[1]))?;
    match args[2].as_str() {
        "art" => {
            let n: Vec<usize> = args[3..7].iter().map(|a| a.parse().unwrap()).collect();
            let ch = |ink| LEGEND.iter().find(|(_, i)| *i == ink).unwrap().0;
            for y in n[1]..=n[3] {
                println!("{y:5} {}", (n[0]..=n[2]).map(|x| ch(board.ink(x, y))).collect::<String>());
            }
        }
        "at" => {
            let c = Components::build(&board)?;
            for place in &args[3..] {
                let (x, y) = xy(place);
                let Some(e) = c.entity_at(x, y) else {
                    println!("{place}: nothing");
                    continue;
                };
                println!("{place}: {}", describe(&board, &c, e));
                for (label, list) in [("reads", &c.reads), ("writes", &c.writes), ("read by", &c.readers), ("written by", &c.writers)] {
                    for other in &list[e as usize] {
                        println!("    {label}: {}", describe(&board, &c, *other));
                    }
                }
            }
        }
        "watch" => {
            let source = std::fs::read_to_string(&args[3]).map_err(|e| e.to_string())?.replace("\r\n", "\n");
            vcb::asm::assemble(&source, &args[3], &mut board, false)?;
            let (from, to): (u64, u64) = (args[4].parse().unwrap(), args[5].parse().unwrap());
            let c = Components::build(&board)?;
            let watched: Vec<(String, u32)> = args[6..]
                .iter()
                .map(|a| {
                    let (place, name) = a.split_once('=').unwrap_or((a, a));
                    let (x, y) = xy(place);
                    (name.to_string(), c.entity_at(x, y).unwrap_or_else(|| panic!("nothing at {place}")))
                })
                .collect();
            let mut engine = Engine::new(&board, c, &Default::default())?;
            let mut last: Vec<bool> = watched.iter().map(|(_, e)| engine.state()[*e as usize]).collect();
            while engine.tick() < to {
                engine.step();
                if engine.tick() < from {
                    last = watched.iter().map(|(_, e)| engine.state()[*e as usize]).collect();
                    continue;
                }
                let now: Vec<bool> = watched.iter().map(|(_, e)| engine.state()[*e as usize]).collect();
                let changes: Vec<String> = watched.iter().zip(&now).zip(&last).filter(|((_, n), l)| n != l).map(|(((name, _), n), _)| format!("{name}={}", *n as u8)).collect();
                if !changes.is_empty() {
                    println!("tick {}: {}", engine.tick(), changes.join(" "));
                }
                last = now;
            }
        }
        // The pixels of one bus (same bus ink, joined edge to edge) and every trace pixel that touches it.
        "bus" => {
            let (x, y) = xy(&args[3]);
            let ink = board.ink(x, y);
            let (mut seen, mut todo, mut taps) = (std::collections::HashSet::from([(x, y)]), vec![(x, y)], Vec::new());
            while let Some((x, y)) = todo.pop() {
                for (dx, dy) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
                    // A Cross passes the bus straight on.
                    let (mut nx, mut ny) = ((x as i64 + dx) as usize, (y as i64 + dy) as usize);
                    while nx < board.width && ny < board.height && board.ink(nx, ny) == vcb::Ink::Cross {
                        (nx, ny) = ((nx as i64 + dx) as usize, (ny as i64 + dy) as usize);
                    }
                    if nx >= board.width || ny >= board.height {
                        continue;
                    }
                    let other = board.ink(nx, ny);
                    if other == ink && seen.insert((nx, ny)) {
                        todo.push((nx, ny));
                    } else if matches!(other, vcb::Ink::Trace(_)) {
                        taps.push((format!("{other:?}"), nx, ny));
                    }
                }
            }
            let (xs, ys): (Vec<usize>, Vec<usize>) = seen.iter().copied().unzip();
            println!("{ink:?}: {} px, x {}..{} y {}..{}", seen.len(), xs.iter().min().unwrap(), xs.iter().max().unwrap(), ys.iter().min().unwrap(), ys.iter().max().unwrap());
            taps.sort();
            taps.dedup();
            for (colour, x, y) in taps {
                println!("    {colour} at {x},{y}");
            }
        }
        other => return Err(format!("unknown command {other}")),
    }
    Ok(())
}
