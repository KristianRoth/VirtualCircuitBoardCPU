//! What a netlist optimisation could remove from the part of a board that v2 simulates.
//!   cargo run --release --example netlist_stats -- <board.vcb>
use std::collections::HashMap;
use vcb::netlist::{Components, Kind};
use vcb::Ink;

fn main() -> Result<(), String> {
    let board = vcb::Board::read(std::path::Path::new(&std::env::args().nth(1).ok_or("board")?))?;
    let c = Components::build(&board)?;
    let engine = vcb::sim2::Engine::new(&board, c.clone(), &vcb::sim::Options { timer_ticks: Some(1000) })?;
    let live = engine.live();
    let n = c.kinds.len();
    let is = |e: usize, k: fn(&Kind) -> bool| live[e] && k(&c.kinds[e]);
    let traces: Vec<usize> = (0..n).filter(|e| is(*e, |k| *k == Kind::Trace)).collect();
    let comps: Vec<usize> = (0..n).filter(|e| is(*e, |k| matches!(k, Kind::Component(_)))).collect();
    let pins = (0..n).filter(|e| is(*e, |k| *k == Kind::Pin)).count();
    println!("simulated: {} entities = {} traces + {} components + {pins} pins", traces.len() + comps.len() + pins, traces.len(), comps.len());

    // Traces by number of writers and readers (simulated readers only).
    let live_readers = |t: usize| c.readers[t].iter().filter(|r| live[**r as usize]).count();
    let single = traces.iter().filter(|t| c.writers[**t].len() == 1).count();
    let single_both = traces.iter().filter(|t| c.writers[**t].len() == 1 && live_readers(**t) == 1).count();
    let no_reader = traces.iter().filter(|t| live_readers(**t) == 0).count();
    println!("traces with one writer: {single} ({:.0}%); of those with one reader too: {single_both}; traces with no simulated reader: {no_reader}", 100.0 * single as f64 / traces.len() as f64);
    let mut by_writers: HashMap<usize, usize> = HashMap::new();
    for t in &traces {
        *by_writers.entry(c.writers[*t].len().min(5)).or_default() += 1;
    }
    let mut w: Vec<_> = by_writers.into_iter().collect();
    w.sort();
    println!("traces by number of writers (5 = five or more): {w:?}");

    // Components by ink and number of inputs.
    let mut by_ink: HashMap<String, (usize, usize)> = HashMap::new();
    for e in &comps {
        if let Kind::Component(ink) = c.kinds[*e] {
            let entry = by_ink.entry(format!("{ink}")).or_default();
            entry.0 += 1;
            entry.1 += (c.reads[*e].len() == 1) as usize;
        }
    }
    let mut inks: Vec<_> = by_ink.into_iter().collect();
    inks.sort_by_key(|(_, (k, _))| std::cmp::Reverse(*k));
    println!("components by ink (count, of which with one input): {inks:?}");

    // Gates that compute the same thing: same ink, same set of input traces. (Latches and clocks only if truly alike.)
    let mut groups: HashMap<(String, Vec<u32>), usize> = HashMap::new();
    for e in &comps {
        if let Kind::Component(ink) = c.kinds[*e] {
            if matches!(ink, Ink::Random) || c.reads[*e].is_empty() {
                continue;
            }
            let mut reads = c.reads[*e].clone();
            reads.sort();
            *groups.entry((format!("{ink}"), reads)).or_default() += 1;
        }
    }
    let duplicates: usize = groups.values().map(|k| k - 1).sum();
    println!("components that duplicate another (same ink, same inputs): {duplicates}");

    // One-input gates: a Buffer, OR, AND or XOR with one input repeats it a tick later; NOT, NOR, NAND, XNOR invert it.
    let one_input = comps.iter().filter(|e| c.reads[**e].len() == 1 && !matches!(c.kinds[**e], Kind::Component(Ink::LatchOn | Ink::LatchOff | Ink::Clock | Ink::Timer | Ink::Random))).count();
    println!("gates with one input (pure one-tick delays or inverters): {one_input}");

    // What can never change: not downstream of a clock, timer, random or VMem pin.
    let mut can_change = vec![false; n];
    let mut todo: Vec<usize> = (0..n).filter(|e| matches!(c.kinds[*e], Kind::Pin | Kind::Component(Ink::Clock | Ink::Timer | Ink::Random))).collect();
    while let Some(e) = todo.pop() {
        if !std::mem::replace(&mut can_change[e], true) {
            let next = if c.kinds[e] == Kind::Trace { &c.readers[e] } else { &c.writes[e] };
            todo.extend(next.iter().map(|x| *x as usize));
        }
    }
    let constant = (0..n).filter(|e| live[*e] && !can_change[*e]).count();
    println!("simulated entities that can never change (constants): {constant}");
    let fan: usize = comps.iter().map(|e| c.reads[*e].len()).sum();
    println!("inputs per simulated component: {:.2}; readers per simulated trace: {:.2}", fan as f64 / comps.len() as f64, traces.iter().map(|t| live_readers(*t)).sum::<usize>() as f64 / traces.len() as f64);
    Ok(())
}
