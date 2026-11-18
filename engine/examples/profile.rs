//! Measurements of v1 for the v2 profile (specs/vcb-engine-v2-log.md). Run from engine/:
//!   cargo run --release --example profile -- <board.vcb> <program.vcbasm> activity <ticks>
//!   cargo run --release --example profile -- <board.vcb> <program.vcbasm> repeats <ticks>
//!   cargo run --release --example profile -- <board.vcb> <program.vcbasm> windows <window ticks>
//!   cargo run --release --example profile -- <board.vcb> <program.vcbasm> bare <ticks>
//! It uses only the engine's public interface, so the numbers are about v1 as it is.

use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;
use vcb::netlist::{Components, Kind};
use vcb::sim::{Engine, Simulation};
use vcb::Board;

/// Only boards with Timer ink use it (the compact sample computer; its timers do not reach the memory).
const OPTIONS: vcb::sim::Options = vcb::sim::Options { timer_ticks: Some(1000) };

/// A fast 64-bit hash of an on/off vector (FNV-style over 8-byte chunks).
fn hash(state: &[bool], seed: u64) -> u64 {
    let mut h = seed ^ 0xcbf2_9ce4_8422_2325;
    for chunk in state.chunks(8) {
        let mut word = 0u64;
        for (i, b) in chunk.iter().enumerate() {
            word |= (*b as u64) << (8 * i);
        }
        h = (h ^ word).wrapping_mul(0x0000_0100_0000_01b3).rotate_left(23);
    }
    h
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let mut board = Board::read(Path::new(&args[1]))?;
    let source = std::fs::read_to_string(&args[2]).map_err(|e| e.to_string())?.replace("\r\n", "\n");
    vcb::asm::assemble(&source, &args[2], &mut board, false)?;
    let c = Components::build(&board)?;
    let n = c.kinds.len();
    let (traces, components) = (c.kinds.iter().filter(|k| **k == Kind::Trace).count(), c.kinds.iter().filter(|k| matches!(k, Kind::Component(_))).count());
    let count: u64 = args[4].parse().unwrap();

    match args[3].as_str() {
        // How much of the circuit is touched per tick.
        "activity" => {
            println!("entities: {n} ({traces} traces, {components} components, {} pins)", n - traces - components);
            let reads: f64 = c.reads.iter().map(|r| r.len()).sum::<usize>() as f64 / components as f64;
            println!("average traces read per component: {reads:.2}");
            let kinds: Vec<Kind> = c.kinds.clone();
            let readers: Vec<Vec<u32>> = c.readers.clone();
            let reads_of: Vec<Vec<u32>> = c.reads.clone();
            let writers_of: Vec<Vec<u32>> = c.writers.clone();
            let mut by_ink: HashMap<String, u64> = HashMap::new();
            let mut engine = Engine::new(&board, c, &OPTIONS)?;
            let mut previous = engine.state().to_vec();
            let (mut trace_changes, mut component_flips, mut evaluations, mut inputs_scanned, mut writer_scans, mut reader_visits) = (0u64, 0u64, 0u64, 0u64, 0u64, 0u64);
            let mut queued = vec![u64::MAX; n];
            let mut per_tick: Vec<u32> = Vec::new();
            let mut ever_changed = vec![false; n];
            let mut ticks = 0u64;
            while ticks < count && !engine.breakpoint_hit() {
                engine.step();
                ticks += 1;
                let state = engine.state();
                let mut evaluated_next = 0u32;
                for e in 0..n {
                    if state[e] == previous[e] {
                        continue;
                    }
                    ever_changed[e] = true;
                    if kinds[e] == Kind::Trace {
                        trace_changes += 1;
                        writer_scans += writers_of[e].len() as u64;
                        // Every reader of a changed trace is evaluated in the next tick (once).
                        for &r in &readers[e] {
                            reader_visits += 1;
                            if queued[r as usize] != ticks {
                                queued[r as usize] = ticks;
                                evaluated_next += 1;
                                inputs_scanned += reads_of[r as usize].len() as u64;
                                if let Kind::Component(ink) = kinds[r as usize] {
                                    *by_ink.entry(format!("{ink}")).or_default() += 1;
                                }
                            }
                        }
                    } else {
                        component_flips += 1;
                    }
                }
                evaluations += evaluated_next as u64;
                per_tick.push(evaluated_next);
                previous.copy_from_slice(state);
            }
            let t = ticks as f64;
            per_tick.sort();
            println!("ticks measured: {ticks}");
            println!("components evaluated per tick: {:.1} mean, {} median, {} at the 99th percentile, {} max = {:.2}% of all components", evaluations as f64 / t, per_tick[per_tick.len() / 2], per_tick[per_tick.len() * 99 / 100], per_tick[per_tick.len() - 1], 100.0 * evaluations as f64 / t / components as f64);
            println!("inputs scanned per tick (in evaluations): {:.1}", inputs_scanned as f64 / t);
            println!("components that flip per tick: {:.1} ({:.0}% of the evaluated ones)", component_flips as f64 / t, 100.0 * component_flips as f64 / evaluations as f64);
            println!("traces that change per tick: {:.1}; writers scanned for them: {:.1}; readers visited: {:.1}", trace_changes as f64 / t, writer_scans as f64 / t, reader_visits as f64 / t);
            let (changed_traces, changed_components) = ((0..n).filter(|e| ever_changed[*e] && kinds[*e] == Kind::Trace).count(), (0..n).filter(|e| ever_changed[*e] && kinds[*e] != Kind::Trace).count());
            println!("entities that changed at least once: {changed_traces} traces, {changed_components} components and pins ({:.0}% of all entities)", 100.0 * (changed_traces + changed_components) as f64 / n as f64);
            let mut inks: Vec<(String, u64)> = by_ink.into_iter().collect();
            inks.sort_by_key(|(_, k)| std::cmp::Reverse(*k));
            let shares: Vec<String> = inks.iter().map(|(ink, k)| format!("{ink} {:.0}%", 100.0 * *k as f64 / evaluations as f64)).collect();
            println!("evaluations by ink: {}", shares.join(", "));
        }
        // How often the circuit is in a state it has been in before, and for how long it then repeats.
        "repeats" => {
            let mut engine = Engine::new(&board, c, &OPTIONS)?;
            // The key of a tick: the state after it and the state before it (which fixes what rose and what is pending).
            let mut seen: HashMap<u64, u64> = HashMap::new();
            let mut keys: Vec<u64> = Vec::with_capacity(count as usize);
            let mut previous = hash(engine.state(), 1);
            let (mut repeated, mut ticks) = (0u64, 0u64);
            let mut run_lengths: Vec<u64> = Vec::new();
            let mut following: Option<(u64, u64)> = None; // (the earlier tick being followed, length so far)
            while ticks < count && !engine.breakpoint_hit() {
                engine.step();
                let now = hash(engine.state(), 1);
                let key = now ^ previous.rotate_left(32);
                previous = now;
                // Is this tick the continuation of an earlier stretch we are following?
                following = match following {
                    Some((earlier, length)) if keys.get(earlier as usize + 1) == Some(&key) => Some((earlier + 1, length + 1)),
                    Some((_, length)) => {
                        run_lengths.push(length);
                        None
                    }
                    None => None,
                };
                if let Some(&earlier) = seen.get(&key) {
                    repeated += 1;
                    if following.is_none() {
                        following = Some((earlier, 1));
                    }
                } else {
                    seen.insert(key, ticks);
                }
                keys.push(key);
                ticks += 1;
            }
            if let Some((_, length)) = following {
                run_lengths.push(length);
            }
            run_lengths.sort();
            let total: u64 = run_lengths.iter().sum();
            println!("ticks measured: {ticks}; different states: {}; ticks in a state seen before: {repeated} ({:.1}%)", seen.len(), 100.0 * repeated as f64 / ticks as f64);
            if !run_lengths.is_empty() {
                let at = |q: usize| run_lengths[(run_lengths.len() - 1) * q / 100];
                println!("stretches that repeat an earlier one: {} (covering {total} ticks); length median {}, 90th percentile {}, longest {}", run_lengths.len(), at(50), at(90), at(100));
                for limit in [16u64, 64, 256, 1024] {
                    let covered: u64 = run_lengths.iter().filter(|l| **l >= limit).sum();
                    println!("    ticks inside repeated stretches of at least {limit}: {:.1}% of all ticks", 100.0 * covered as f64 / ticks as f64);
                }
            }
        }
        // Speed of the bare step loop in consecutive windows, to the Breakpoint: does the start resemble the rest?
        "windows" => {
            let mut engine = Engine::new(&board, c, &OPTIONS)?;
            let mut line = Vec::new();
            loop {
                let started = Instant::now();
                let mut done = 0u64;
                while done < count && !engine.breakpoint_hit() {
                    engine.step();
                    done += 1;
                }
                line.push(format!("{:.2}", done as f64 / started.elapsed().as_secs_f64() / 1e6));
                if engine.breakpoint_hit() {
                    break;
                }
            }
            println!("million ticks per second per window of {count} ticks, to the Breakpoint at tick {}:\n{}", engine.tick(), line.join(" "));
        }
        // The step loop with nothing around it.
        "bare" => {
            let mut engine = Engine::new(&board, c, &OPTIONS)?;
            let started = Instant::now();
            let mut done = 0u64;
            while done < count && !engine.breakpoint_hit() {
                engine.step();
                done += 1;
            }
            let seconds = started.elapsed().as_secs_f64();
            println!("bare step loop: {done} ticks in {seconds:.3} s = {:.2} million ticks per second ({:.0} ns per tick)", done as f64 / seconds / 1e6, seconds * 1e9 / done as f64);
        }
        other => return Err(format!("unknown measurement {other}")),
    }
    Ok(())
}
