//! Before building replay (v2 attempt 8): how often would it hit? A node is the circuit's state at the end of
//! a tick with an event (VMem load, write, Breakpoint). From a node the circuit is deterministic up to the next
//! event; if that is a load, what follows also depends on the word loaded. This counts, over a run, how many
//! different nodes there are and how many steps (node, loaded word) have been seen before.
//!   cargo run --release --example replay_estimate -- <board.vcb> <program.vcbasm or -> <ticks>
use std::collections::{HashMap, HashSet};
use std::path::Path;
use vcb::netlist::Components;
use vcb::sim::Simulation;

fn hash(state: &[bool]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
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
    let mut board = vcb::Board::read(Path::new(&args[1]))?;
    if args[2] != "-" {
        let source = std::fs::read_to_string(&args[2]).map_err(|e| e.to_string())?.replace("\r\n", "\n");
        vcb::asm::assemble(&source, &args[2], &mut board, true)?;
    } else {
        let text = board.assembly.clone();
        vcb::asm::assemble(&text, "board", &mut board, true)?;
    }
    let limit: u64 = args[3].parse().unwrap();
    let interval = board.settings.clock_interval as u64;
    let c = Components::build(&board)?;
    let mut engine = vcb::sim2::Engine::new(&board, c, &vcb::sim::Options { timer_ticks: Some(1000) })?;
    let mut nodes: HashSet<u64> = HashSet::new();
    let mut steps: HashSet<(u64, u32)> = HashSet::new();
    let mut by_window: HashMap<u64, (u64, u64, u64)> = HashMap::new(); // per 100M ticks: events, known steps, ticks inside known steps
    let (mut node, mut node_tick) = (0u64, 0u64);
    let (mut events, mut known, mut known_ticks) = (0u64, 0u64, 0u64);
    while engine.tick() < limit && !engine.breakpoint_hit() {
        engine.advance(limit);
        let (fetched, _) = engine.vmem_access();
        // The word this event loaded, if it was a load: it decides the state after the event.
        let word = fetched.map_or(0, |a| engine.vmem()[a]);
        let seen = !steps.insert((node, word));
        let ticks = engine.tick() - node_tick;
        events += 1;
        known += seen as u64;
        known_ticks += if seen { ticks } else { 0 };
        let w = by_window.entry(engine.tick() / 100_000_000).or_default();
        *w = (w.0 + 1, w.1 + seen as u64, w.2 + if seen { ticks } else { 0 });
        // The state after the event, with the clock position, as the next node.
        node = hash(engine.state()) ^ (engine.tick() % interval).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        node_tick = engine.tick();
        nodes.insert(node);
    }
    let t = engine.tick();
    println!("ticks {t}, events {events} (one per {:.0} ticks), different nodes {}, different steps {}", t as f64 / events as f64, nodes.len(), steps.len());
    println!("steps seen before: {:.1}% of the events, covering {:.1}% of the ticks", 100.0 * known as f64 / events as f64, 100.0 * known_ticks as f64 / t as f64);
    let mut windows: Vec<_> = by_window.into_iter().collect();
    windows.sort();
    let line: Vec<String> = windows.iter().map(|(_, (_, _, k))| format!("{:.0}", *k as f64 / 1e6)).collect();
    println!("ticks covered by known steps per window of 100 million ticks, in percent: {}", line.join(" "));
    Ok(())
}
