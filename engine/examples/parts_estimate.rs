//! Before building part-level skipping: how much of a board's activity could it skip?
//!
//! The simulated circuit is split into parts by rules that use only the netlist (no knowledge of the board):
//!   level "T=n": cut every trace that more than n components share; what stays connected is a part;
//!   level "control": the finest parts, merged when they are steered by exactly the same widely shared traces.
//! A part is steered from outside only through its inputs. Whenever an input changes, the part starts a
//! **burst**: what it does from then until its inputs change again follows from its own signals, its inputs
//! and what just changed. If that same situation was seen before, the burst could be replayed, not simulated.
//! This tool runs the reference engine and counts, per level and for the levels together (the largest part
//! that recognises its situation wins), the share of all flips that lie in bursts seen before.
//!
//!   cargo run --release --example parts_estimate -- <board.vcb> <program.vcbasm or -> [ticks] [levels]
//! `levels`: thresholds coarse to fine, for example 32,8 (and the word control for that level). Default: the
//! powers of two under the largest degree, control, 3. A level alone shows what that level is worth by itself.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use vcb::netlist::{Components, Kind};
use vcb::sim::Simulation;

const NONE: u32 = u32::MAX;

struct Level {
    name: String,
    /// Per entity: its part, or NONE (not simulated, or a cut trace).
    part: Vec<u32>,
    parts: usize,
    /// Per entity: the other parts that read it directly.
    feeds: Vec<Vec<u32>>,
    inputs: Vec<u32>,
    // Per part, while running:
    state: Vec<u64>,
    input_state: Vec<u64>,
    changed: Vec<u64>,
    input_changed: Vec<u64>,
    event: Vec<bool>,
    seen_burst: Vec<bool>,
    known: HashSet<u64>,
    // Totals:
    events: u64,
    events_seen: u64,
    events_unsettled: u64,
    flips_in_parts: u64,
    flips_seen: u64,
}

fn mix(a: u64, b: u64) -> u64 {
    (a ^ b.rotate_left(29)).wrapping_mul(0x9e37_79b9_7f4a_7c15).rotate_left(31) ^ b
}

/// Connected groups of the simulated entities when the traces in `cut` are taken out.
fn connected(n: usize, simulated: &[bool], cut: &[bool], c: &Components) -> (Vec<u32>, usize) {
    let mut part = vec![NONE; n];
    let mut parts = 0;
    for start in 0..n {
        if !simulated[start] || cut[start] || part[start] != NONE {
            continue;
        }
        let mut todo = vec![start];
        part[start] = parts as u32;
        while let Some(e) = todo.pop() {
            let next: Vec<u32> = if c.kinds[e] == Kind::Trace { c.readers[e].iter().chain(&c.writers[e]).copied().collect() } else { c.reads[e].iter().chain(&c.writes[e]).copied().collect() };
            for x in next {
                let x = x as usize;
                if simulated[x] && !cut[x] && part[x] == NONE {
                    part[x] = parts as u32;
                    todo.push(x);
                }
            }
        }
        parts += 1;
    }
    (part, parts)
}

fn level(name: String, part: Vec<u32>, parts: usize, c: &Components, simulated: &[bool]) -> Level {
    let n = part.len();
    let mut feeds: Vec<Vec<u32>> = vec![Vec::new(); n];
    let mut inputs = vec![0u32; parts];
    for e in 0..n {
        if part[e] == NONE {
            continue;
        }
        // What e reads directly: a component its traces, a trace its writers.
        for &x in if c.kinds[e] == Kind::Trace { &c.writers[e] } else { &c.reads[e] } {
            if simulated[x as usize] && part[x as usize] != part[e] && !feeds[x as usize].contains(&part[e]) {
                feeds[x as usize].push(part[e]);
                inputs[part[e] as usize] += 1;
            }
        }
    }
    Level { name, part, parts, feeds, inputs, state: vec![0; parts], input_state: vec![0; parts], changed: vec![0; parts], input_changed: vec![0; parts], event: vec![false; parts], seen_burst: vec![false; parts], known: HashSet::new(), events: 0, events_seen: 0, events_unsettled: 0, flips_in_parts: 0, flips_seen: 0 }
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let mut board = vcb::Board::read(Path::new(&args[1]))?;
    let source = if args[2] == "-" { board.assembly.clone() } else { std::fs::read_to_string(&args[2]).map_err(|e| e.to_string())?.replace("\r\n", "\n") };
    vcb::asm::assemble(&source, &args[2], &mut board, true)?;
    let ticks: u64 = args.get(3).map_or(3_000_000, |t| t.parse().unwrap());
    let options = vcb::sim::Options { timer_ticks: Some(1000) };
    let c = Components::build(&board)?;
    let n = c.kinds.len();
    let live = vcb::sim2::Engine::new(&board, c.clone(), &options)?.live().to_vec();
    // Simulated: what v2 simulates, with the one-writer traces it folds into their writers.
    let simulated: Vec<bool> = (0..n).map(|e| live[e] || (c.kinds[e] == Kind::Trace && c.writers[e].len() == 1 && live[c.writers[e][0] as usize])).collect();
    let degree: Vec<usize> = (0..n).map(|e| if c.kinds[e] == Kind::Trace && simulated[e] { c.readers[e].iter().chain(&c.writers[e]).filter(|x| simulated[**x as usize]).count() } else { 0 }).collect();

    // The levels, coarse to fine: thresholds are powers of two, the same for every board.
    let mut levels: Vec<Level> = Vec::new();
    let max_degree = *degree.iter().max().unwrap();
    let chosen: Option<Vec<String>> = args.get(4).map(|l| l.split(',').map(str::to_string).collect());
    let mut thresholds: Vec<usize> = match &chosen {
        Some(list) => list.iter().filter_map(|t| t.parse().ok()).collect(),
        None => (2..7).rev().map(|k| 1usize << k).filter(|t| *t < max_degree).chain([3]).collect(),
    };
    thresholds.sort_by(|a, b| b.cmp(a));
    for t in &thresholds {
        let cut: Vec<bool> = degree.iter().map(|d| *d > *t).collect();
        let (part, parts) = connected(n, &simulated, &cut, &c);
        levels.push(level(format!("T={t}"), part, parts, &c, &simulated));
    }
    // "control": the finest parts merged when the same widely shared traces (degree over 8) steer them.
    if chosen.as_ref().is_none_or(|list| list.iter().any(|l| l == "control")) && !levels.is_empty() {
        let fine = levels.last().unwrap();
        let mut signature: Vec<Vec<u32>> = vec![Vec::new(); fine.parts];
        for x in 0..n {
            if degree[x] > 8 {
                for p in &fine.feeds[x] {
                    signature[*p as usize].push(x as u32);
                }
            }
        }
        let mut group: HashMap<Vec<u32>, u32> = HashMap::new();
        let mut merged_into: Vec<u32> = Vec::new();
        let mut parts = 0u32;
        for p in 0..fine.parts {
            let id = if signature[p].is_empty() {
                parts += 1;
                parts - 1
            } else {
                *group.entry(signature[p].clone()).or_insert_with(|| {
                    parts += 1;
                    parts - 1
                })
            };
            merged_into.push(id);
        }
        let part: Vec<u32> = fine.part.iter().map(|p| if *p == NONE { NONE } else { merged_into[*p as usize] }).collect();
        let at = levels.len() - 1;
        levels.insert(at, level("control".into(), part, parts as usize, &c, &simulated));
    }

    // Random numbers per entity, so that a part's signals hash as the XOR of those that are on.
    let mut seed = 0x1234_5678_9abc_def1u64;
    let mut random = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let z: Vec<(u64, u64)> = (0..n).map(|_| (random(), random())).collect();

    let mut engine = vcb::sim::Engine::new(&board, c.clone(), &options)?;
    let mut previous = engine.state().to_vec();
    for l in &mut levels {
        for e in 0..n {
            if previous[e] && simulated[e] {
                if l.part[e] != NONE {
                    l.state[l.part[e] as usize] ^= z[e].0;
                }
                for q in &l.feeds[e] {
                    l.input_state[*q as usize] ^= z[e].0;
                }
            }
        }
    }
    let (mut flips, mut flips_cut, mut flips_self) = (0u64, 0u64, 0u64);
    let mut covered_by = vec![0u64; levels.len()];
    let mut changed: Vec<usize> = Vec::new();
    let mut run = 0u64;
    while run < ticks && !engine.breakpoint_hit() {
        engine.step();
        run += 1;
        let state = engine.state();
        changed.clear();
        changed.extend((0..n).filter(|e| simulated[*e] && state[*e] != previous[*e]));
        // Each flip: which level, coarsest first, has it inside a burst seen before?
        for &e in &changed {
            flips += 1;
            if levels.iter().all(|l| l.part[e] == NONE) {
                flips_cut += 1;
                continue;
            }
            if levels.iter().rev().find(|l| l.part[e] != NONE).is_some_and(|l| l.inputs[l.part[e] as usize] == 0) { // judged at the finest level
                flips_self += 1; // a part without inputs runs by itself: a schedule, not a burst
                continue;
            }
            let mut first = true;
            for (i, l) in levels.iter_mut().enumerate() {
                let p = l.part[e];
                if p == NONE {
                    continue;
                }
                l.flips_in_parts += 1;
                if l.seen_burst[p as usize] {
                    l.flips_seen += 1;
                    if std::mem::take(&mut first) {
                        covered_by[i] += 1;
                    }
                }
            }
        }
        // Then the new situation of every part that had an input change.
        for l in &mut levels {
            for &e in &changed {
                let p = l.part[e];
                if p != NONE {
                    l.state[p as usize] ^= z[e].0;
                    l.changed[p as usize] ^= z[e].1;
                }
                for &q in &l.feeds[e] {
                    l.input_state[q as usize] ^= z[e].0;
                    l.input_changed[q as usize] ^= z[e].1;
                    l.event[q as usize] = true;
                }
            }
            for &e in &changed {
                for k in 0..l.feeds[e].len() {
                    let q = l.feeds[e][k] as usize;
                    if !std::mem::take(&mut l.event[q]) {
                        continue;
                    }
                    let key = mix(mix(mix(mix(q as u64, l.state[q]), l.input_state[q]), l.changed[q]), l.input_changed[q]);
                    let seen = !l.known.insert(key);
                    l.events += 1;
                    l.events_seen += seen as u64;
                    l.events_unsettled += (l.changed[q] != 0) as u64;
                    l.seen_burst[q] = seen;
                    l.input_changed[q] = 0;
                }
            }
            for &e in &changed {
                if l.part[e] != NONE {
                    l.changed[l.part[e] as usize] = 0;
                }
            }
        }
        previous.copy_from_slice(state);
    }

    let pct = |a: u64, b: u64| 100.0 * a as f64 / b.max(1) as f64;
    println!("{} simulated entities, {run} ticks, {:.2} flips per tick", simulated.iter().filter(|s| **s).count(), flips as f64 / run as f64);
    println!("flips in parts without inputs (self-running, a schedule): {:.1}%; on traces cut at every level: {:.1}%", pct(flips_self, flips), pct(flips_cut, flips));
    // "net": flips saved per tick if a lookup costs as much as simulating 8 flips (an assumption, not a measurement).
    println!("{:<9}{:>7}{:>10}{:>12}{:>13}{:>14}{:>12}{:>10}{:>16}", "level", "parts", "largest", "events/tick", "events seen", "flips skipped", "per lookup", "net", "situations kept");
    for l in &levels {
        let mut size = vec![0usize; l.parts];
        for p in l.part.iter().filter(|p| **p != NONE) {
            size[*p as usize] += 1;
        }
        let (per_tick, lookups) = (l.flips_seen as f64 / run as f64, l.events as f64 / run as f64);
        println!("{:<9}{:>7}{:>10}{:>12.3}{:>12.1}%{:>13.1}%{:>12.1}{:>10.2}{:>16}", l.name, l.parts, size.iter().max().unwrap_or(&0), lookups, pct(l.events_seen, l.events), pct(l.flips_seen, flips), l.flips_seen as f64 / l.events.max(1) as f64, per_tick - 8.0 * lookups, l.known.len());
    }
    let total: u64 = covered_by.iter().sum();
    let by: Vec<String> = levels.iter().zip(&covered_by).map(|(l, k)| format!("{} {:.1}%", l.name, pct(*k, flips))).collect();
    println!("the levels together, the largest part that knows its situation first: {:.1}% of all flips skipped ({})", pct(total, flips), by.join(", "));
    println!("left to simulate: {:.1}% of all flips (self-running parts not counted as left)", pct(flips - total - flips_self, flips));
    Ok(())
}
