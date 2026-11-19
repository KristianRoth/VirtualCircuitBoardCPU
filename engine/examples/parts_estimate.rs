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
    /// The same counted only inside steps that whole-board replay does not know: what parts add on top of it.
    /// `step_*` collect the current step; they count (and the situations are kept) only if the step was a miss.
    step_keys: Vec<u64>,
    step_events: u64,
    step_flips_seen: u64,
    miss_events: u64,
    miss_flips_seen: u64,
    /// Per part: the bits a table would be indexed by: its simulated signals and its inputs, each with
    /// "changed in the last tick" (which is what decides the pending work).
    index_bits: Vec<u32>,
    /// Per limit of index bits (16, 24): inside missed steps, the events of parts that small and all their flips
    /// (a table knows every situation, so nothing of such a part needs simulating).
    step_small: [(u64, u64); 2],
    miss_small: [(u64, u64); 2],
    /// Per part, inside missed steps: (input events, events whose situation was known, flips in known bursts, all flips).
    p_step: Vec<[u64; 4]>,
    p_miss: Vec<[u64; 4]>,
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
    Level { name, part, parts, feeds, inputs, state: vec![0; parts], input_state: vec![0; parts], changed: vec![0; parts], input_changed: vec![0; parts], event: vec![false; parts], seen_burst: vec![false; parts], known: HashSet::new(), events: 0, events_seen: 0, events_unsettled: 0, flips_in_parts: 0, flips_seen: 0, step_keys: Vec::new(), step_events: 0, step_flips_seen: 0, miss_events: 0, miss_flips_seen: 0, index_bits: Vec::new(), step_small: [(0, 0); 2], miss_small: [(0, 0); 2], p_step: vec![[0; 4]; parts], p_miss: vec![[0; 4]; parts] }
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

    for l in &mut levels {
        let mut bits = vec![0u32; l.parts];
        for e in (0..n).filter(|e| live[*e] && l.part[*e] != NONE) {
            bits[l.part[e] as usize] += 2;
        }
        l.index_bits = (0..l.parts).map(|p| bits[p] + 2 * l.inputs[p]).collect();
    }
    const SMALL: [u32; 2] = [16, 24];

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
    let mut steps: HashSet<(u64, u32)> = HashSet::new();
    let (mut node, mut step_flips, mut flips_replayed) = (0u64, 0u64, 0u64);
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
                l.p_step[p as usize][3] += 1;
                l.p_step[p as usize][2] += l.seen_burst[p as usize] as u64;
                for (k, limit) in SMALL.iter().enumerate() {
                    l.step_small[k].1 += (l.index_bits[p as usize] <= *limit) as u64;
                }
                if l.seen_burst[p as usize] {
                    l.flips_seen += 1;
                    l.step_flips_seen += 1;
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
                    // A situation counts as known if a step that was really simulated has shown it.
                    let seen = l.known.contains(&key) || l.step_keys.contains(&key);
                    l.step_keys.push(key);
                    l.step_events += 1;
                    l.p_step[q][0] += 1;
                    l.p_step[q][1] += seen as u64;
                    for (k, limit) in SMALL.iter().enumerate() {
                        l.step_small[k].0 += (l.index_bits[q] <= *limit) as u64;
                    }
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
        step_flips += changed.len() as u64;
        let (fetched, written) = engine.vmem_access();
        if engine.breakpoint_hit() || fetched.is_some() || written.is_some() {
            // Whole-board replay as v2 has it: a step is known if this state led here with this loaded word before.
            let word = fetched.map_or(0, |a| engine.vmem()[a]);
            let replayed = !steps.insert((node, word));
            if replayed {
                flips_replayed += step_flips;
            }
            for l in &mut levels {
                if replayed {
                    // The parts saw nothing of this step: they do not know their situation until their next event.
                    l.seen_burst.fill(false);
                } else {
                    l.known.extend(l.step_keys.iter());
                    l.miss_events += l.step_events;
                    l.miss_flips_seen += l.step_flips_seen;
                    for k in 0..2 {
                        l.miss_small[k] = (l.miss_small[k].0 + l.step_small[k].0, l.miss_small[k].1 + l.step_small[k].1);
                    }
                }
                l.step_small = [(0, 0); 2];
                for (total, step) in l.p_miss.iter_mut().zip(l.p_step.iter_mut()) {
                    if !replayed {
                        for k in 0..4 {
                            total[k] += step[k];
                        }
                    }
                    *step = [0; 4];
                }
                l.step_keys.clear();
                (l.step_events, l.step_flips_seen) = (0, 0);
            }
            step_flips = 0;
            let mut h = 0xcbf2_9ce4_8422_2325u64;
            for e in (0..n).filter(|e| simulated[*e] && state[*e]) {
                h ^= z[e].0;
            }
            node = mix(h, engine.tick() % 64); // 64: the period of nothing in particular; a stand-in for the schedule position
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
    let missed = flips - flips_replayed;
    println!();
    println!("on top of whole-board replay: it already skips {:.1}% of the flips; {:.2} flips per tick are left in its misses", pct(flips_replayed, flips), missed as f64 / run as f64);
    println!("{:<9}{:>22}{:>14}{:>12}{:>10}", "level", "of the missed flips", "lookups/tick", "per lookup", "net");
    for l in &levels {
        let (per_tick, lookups) = (l.miss_flips_seen as f64 / run as f64, l.miss_events as f64 / run as f64);
        println!("{:<9}{:>21.1}%{:>14.3}{:>12.1}{:>10.2}", l.name, pct(l.miss_flips_seen, missed), lookups, l.miss_flips_seen as f64 / l.miss_events.max(1) as f64, per_tick - 8.0 * lookups);
    }
    println!();
    println!("small parts read from a table (index = signals and inputs, each with 'changed last tick'), inside the misses:");
    println!("{:<9}{:>8}{:>18}{:>20}{:>14}{:>12}", "level", "bits <=", "parts that small", "of the missed flips", "lookups/tick", "per lookup");
    for l in &levels {
        for (k, limit) in SMALL.iter().enumerate() {
            let (events, in_them) = l.miss_small[k];
            println!("{:<9}{:>8}{:>10} of {:<5}{:>19.1}%{:>14.3}{:>12.2}", l.name, limit, l.index_bits.iter().filter(|b| **b <= *limit).count(), l.parts, pct(in_them, missed), events as f64 / run as f64, in_them as f64 / events.max(1) as f64);
        }
    }

    // The tree with a depth that follows the parts (the user's idea): a part is used as a unit where that pays,
    // split further where its pieces pay more, and left to the plain simulation where nothing pays. "Pays" is
    // flips saved minus what the lookups cost, with a lookup counted as COST flips. Chosen from the root down,
    // each part by its own numbers; the "control" level does not nest and is left out.
    let tree: Vec<&Level> = levels.iter().filter(|l| l.name != "control").collect();
    for cost in [4.0f64, 8.0, 16.0] {
        // best[i][p]: (net flips per tick, lookups per tick, flips skipped per tick, units) for the subtree of part p of level i.
        let mut best: Vec<Vec<(f64, f64, f64, Vec<(usize, usize)>)>> = Vec::new();
        for i in (0..tree.len()).rev() {
            let l = tree[i];
            let mut children: Vec<Vec<usize>> = vec![Vec::new(); l.parts];
            if i + 1 < tree.len() {
                let mut parent = vec![usize::MAX; tree[i + 1].parts];
                for e in 0..n {
                    if tree[i + 1].part[e] != NONE && l.part[e] != NONE {
                        parent[tree[i + 1].part[e] as usize] = l.part[e] as usize;
                    }
                }
                for (child, p) in parent.iter().enumerate().filter(|(_, p)| **p != usize::MAX) {
                    children[*p].push(child);
                }
            }
            let below = best.last();
            let row: Vec<(f64, f64, f64, Vec<(usize, usize)>)> = (0..l.parts)
                .map(|p| {
                    let [events, _, flips_known, _] = l.p_miss[p];
                    let own = (flips_known as f64 / run as f64 - cost * events as f64 / run as f64, events as f64 / run as f64, flips_known as f64 / run as f64, vec![(i, p)]);
                    let mut split = (0.0, 0.0, 0.0, Vec::new());
                    if let Some(below) = below {
                        for c in &children[p] {
                            split = (split.0 + below[*c].0, split.1 + below[*c].1, split.2 + below[*c].2, [split.3, below[*c].3.clone()].concat());
                        }
                    }
                    let plain = (0.0, 0.0, 0.0, Vec::new());
                    [own, split, plain].into_iter().max_by(|a, b| a.0.total_cmp(&b.0)).unwrap()
                })
                .collect();
            best.push(row);
        }
        let top = best.last().unwrap();
        let (net, lookups, skipped): (f64, f64, f64) = top.iter().fold((0.0, 0.0, 0.0), |a, b| (a.0 + b.0, a.1 + b.1, a.2 + b.2));
        let units: Vec<(usize, usize)> = top.iter().flat_map(|b| b.3.clone()).collect();
        let by_level: Vec<String> = (0..tree.len()).map(|i| (tree[i].name.clone(), units.iter().filter(|u| u.0 == i).count())).filter(|(_, k)| *k > 0).map(|(name, k)| format!("{k} at {name}")).collect();
        println!();
        println!("the tree with its depth following the parts, a lookup counted as {cost} flips: {} units ({})", units.len(), by_level.join(", "));
        println!("    skips {:.1}% of the missed flips with {:.3} lookups per tick ({:.1} flips per lookup); net {:.2} of the {:.2} flips per tick left", 100.0 * skipped * run as f64 / missed.max(1) as f64, lookups, skipped / lookups.max(1e-9), net, missed as f64 / run as f64);
        if cost == 8.0 {
            let mut shown = units.clone();
            shown.sort_by(|a, b| tree[b.0].p_miss[b.1][2].cmp(&tree[a.0].p_miss[a.1][2]));
            println!("    {:<8}{:>9}{:>8}{:>14}{:>13}{:>16}{:>12}", "level", "entities", "inputs", "lookups/tick", "known", "of missed flips", "per lookup");
            for (i, p) in shown.into_iter().take(12) {
                let [events, known, flips_known, _] = tree[i].p_miss[p];
                let size = tree[i].part.iter().filter(|x| **x == p as u32).count();
                println!("    {:<8}{:>9}{:>8}{:>14.3}{:>12.1}%{:>15.1}%{:>12.1}", tree[i].name, size, tree[i].inputs[p], events as f64 / run as f64, pct(known, events), pct(flips_known, missed), flips_known as f64 / events.max(1) as f64);
            }
        }
    }

    // The same tree fixed before the run, by size alone (the user's question): split a part while it has more
    // than `largest` entities, use what comes out as a unit if it has at least `smallest`, simulate the rest.
    let sizes: Vec<Vec<usize>> = tree.iter().map(|l| {
        let mut size = vec![0usize; l.parts];
        for p in l.part.iter().filter(|p| **p != NONE) {
            size[*p as usize] += 1;
        }
        size
    }).collect();
    let children_of = |i: usize| {
        let mut children: Vec<Vec<usize>> = vec![Vec::new(); tree[i].parts];
        if i + 1 < tree.len() {
            let mut seen = vec![false; tree[i + 1].parts];
            for e in 0..n {
                let (p, c) = (tree[i].part[e], tree[i + 1].part[e]);
                if p != NONE && c != NONE && !std::mem::replace(&mut seen[c as usize], true) {
                    children[p as usize].push(c as usize);
                }
            }
        }
        children
    };
    let children: Vec<Vec<Vec<usize>>> = (0..tree.len()).map(children_of).collect();
    println!();
    println!("the tree fixed before the run by size alone (a lookup counted as 8 flips):");
    println!("    {:>9}{:>9}{:>7}{:>22}{:>14}{:>12}{:>8}", "largest", "smallest", "units", "of the missed flips", "lookups/tick", "per lookup", "net");
    for largest in [4096usize, 1024, 512, 256, 128, 64] {
        for smallest in [16usize, 32, 64] {
            if smallest > largest {
                continue;
            }
            let (mut units, mut events, mut flips_known) = (0usize, 0u64, 0u64);
            let mut todo: Vec<(usize, usize)> = (0..tree[0].parts).map(|p| (0, p)).collect();
            while let Some((i, p)) = todo.pop() {
                if sizes[i][p] > largest && !children[i][p].is_empty() {
                    todo.extend(children[i][p].iter().map(|c| (i + 1, *c)));
                } else if sizes[i][p] >= smallest && sizes[i][p] <= largest {
                    units += 1;
                    events += tree[i].p_miss[p][0];
                    flips_known += tree[i].p_miss[p][2];
                }
            }
            let (skipped, lookups) = (flips_known as f64 / run as f64, events as f64 / run as f64);
            println!("    {:>9}{:>9}{:>7}{:>21.1}%{:>14.3}{:>12.1}{:>8.2}", largest, smallest, units, pct(flips_known, missed), lookups, flips_known as f64 / events.max(1) as f64, skipped - 8.0 * lookups);
        }
    }
    Ok(())
}
