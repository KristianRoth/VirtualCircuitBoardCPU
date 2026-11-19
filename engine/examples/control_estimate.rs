//! Step 8 of v2.1 (specs/vcb-engine-v2.1.md): if a step's control were replayed and only its data simulated,
//! how much of the work in the steps replay misses today would go away?
//!
//! A step runs from one VMem event to the next. Steps are grouped into **contexts**: the same length, the
//! same kind of event at the end and the same address loaded there (on a CPU: the same place in the program
//! and in the instruction). Within a context, a flip that every step makes (the same signal at the same tick
//! of the step) is **control**: it does not depend on the data. A flip that only some steps make is **data**.
//! An engine that knew a context could apply its control flips from a record and would have to simulate only
//! the data flips. This counts both, over the steps whose whole state was not seen before (what replay
//! simulates today). It is the best case: it takes the context for granted and charges nothing for finding it.
//!
//!   cargo run --release --example control_estimate -- <board.vcb> <program.vcbasm or -> [ticks]
use std::collections::{HashMap, HashSet};
use std::path::Path;
use vcb::netlist::{Components, Kind};
use vcb::sim::Simulation;
use vcb::Ink;

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let mut board = vcb::Board::read(Path::new(&args[1]))?;
    let source = if args[2] == "-" { board.assembly.clone() } else { std::fs::read_to_string(&args[2]).map_err(|e| e.to_string())?.replace("\r\n", "\n") };
    vcb::asm::assemble(&source, &args[2], &mut board, true)?;
    let ticks: u64 = args.get(3).map_or(5_000_000, |t| t.parse().unwrap());
    let options = vcb::sim::Options { timer_ticks: Some(1000) };
    let c = Components::build(&board)?;
    let n = c.kinds.len();
    // What v2 simulates (the self-running part is played from a schedule there, so it is left out here too).
    let live = vcb::sim2::Engine::new(&board, c.clone(), &options)?.live().to_vec();
    let sim: Vec<bool> = (0..n).map(|e| live[e] && c.kinds[e] != Kind::Component(Ink::Clock)).collect();
    let mut seed = 0x1234_5678_9abc_def1u64;
    let z: Vec<u64> = (0..n).map(|_| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    }).collect();
    let mut engine = vcb::sim::Engine::new(&board, c.clone(), &options)?;
    let mut previous = engine.state().to_vec();
    let mut whole: u64 = (0..n).filter(|e| live[*e] && previous[*e]).fold(0, |h, e| h ^ z[e]);
    // Per step: its context, whether replay would have known it, and its flips as (tick of the step, entity).
    let mut steps: Vec<((u64, u8, u32), bool, Vec<(u32, u32)>)> = Vec::new();
    let mut flips: Vec<(u32, u32)> = Vec::new();
    let mut seen: HashSet<(u64, u32)> = HashSet::new();
    let (mut start_tick, mut start_whole) = (0u64, whole);
    let mut run = 0u64;
    while run < ticks && !engine.breakpoint_hit() {
        engine.step();
        run += 1;
        let state = engine.state();
        for e in 0..n {
            if state[e] != previous[e] {
                if live[e] {
                    whole ^= z[e];
                }
                if sim[e] {
                    flips.push(((engine.tick() - start_tick) as u32, e as u32));
                }
            }
        }
        let (fetched, wrote) = engine.vmem_access();
        if fetched.is_some() || wrote.is_some() || engine.breakpoint_hit() {
            let word = fetched.map_or(0, |a| engine.vmem()[a]);
            let known = !seen.insert((start_whole, word));
            let context = (engine.tick() - start_tick, fetched.is_some() as u8 * 2 + wrote.is_some() as u8, fetched.map_or(u32::MAX, |a| a as u32));
            steps.push((context, known, std::mem::take(&mut flips)));
            start_tick = engine.tick();
            start_whole = whole;
        }
        previous.copy_from_slice(state);
    }
    // Per context: how many steps, and how often each flip occurs.
    let mut contexts: HashMap<(u64, u8, u32), (u32, HashMap<(u32, u32), u32>)> = HashMap::new();
    for (context, _, flips) in &steps {
        let entry = contexts.entry(*context).or_default();
        entry.0 += 1;
        for f in flips {
            *entry.1.entry(*f).or_default() += 1;
        }
    }
    let (mut missed, mut missed_flips, mut control, mut alone, mut alone_flips) = (0u64, 0u64, 0u64, 0u64, 0u64);
    let mut all_flips = 0u64;
    for (context, known, flips) in &steps {
        all_flips += flips.len() as u64;
        if *known {
            continue;
        }
        missed += 1;
        missed_flips += flips.len() as u64;
        let (count, per_flip) = &contexts[context];
        if *count == 1 {
            // The only step of its context: nothing to compare it with, so nothing counts as control.
            alone += 1;
            alone_flips += flips.len() as u64;
            continue;
        }
        control += flips.iter().filter(|f| per_flip[f] == *count).count() as u64;
    }
    let pct = |a: u64, b: u64| 100.0 * a as f64 / b.max(1) as f64;
    println!("{run} ticks, {} steps in {} contexts; replay today would simulate {missed} of them ({:.1}% of the flips)", steps.len(), contexts.len(), pct(missed_flips, all_flips));
    println!("in those simulated steps: {:.0} flips per step; control (the same in every step of the context) {:.1}%, data {:.1}%, in a context seen only once {:.1}%", missed_flips as f64 / missed.max(1) as f64, pct(control, missed_flips), pct(missed_flips - control - alone_flips, missed_flips), pct(alone_flips, missed_flips));
    println!("steps that are the only one of their context: {alone}");

    // Second part: a fixed split made from what was just seen. An entity is DATA if any of its flips was not the
    // same in every step of its context; everything else is CONTROL. A control part that is replayed tick by
    // tick (its state looked up, not simulated) still has to hand its flips to the data part where data reads
    // them, and has to look at every flip of data that it reads. Run again and count those, in the steps that
    // replay would simulate.
    let mut data = vec![false; n];
    for (count, per_flip) in contexts.values() {
        if *count > 1 {
            for ((_, e), k) in per_flip {
                data[*e as usize] |= k != count;
            }
        }
    }
    let control_entities = (0..n).filter(|e| sim[*e] && !data[*e]).count();
    let data_entities = (0..n).filter(|e| sim[*e] && data[*e]).count();
    let readers = |y: usize| if c.kinds[y] == Kind::Trace { &c.readers[y] } else { &c.writes[y] };
    // Does a control entity read e (directly, or through a trace only e's kind writes)? Does a data entity?
    let read_by = |e: usize, want_data: bool| readers(e).iter().any(|r| sim[*r as usize] && data[*r as usize] == want_data);
    let mut engine = vcb::sim::Engine::new(&board, c.clone(), &options)?;
    let mut previous = engine.state().to_vec();
    let mut whole: u64 = (0..n).filter(|e| live[*e] && previous[*e]).fold(0, |h, e| h ^ z[e]);
    let mut control_state: u64 = (0..n).filter(|e| sim[*e] && !data[*e] && previous[*e]).fold(0, |h, e| h ^ z[e]);
    let mut seen: HashSet<(u64, u32)> = HashSet::new();
    let mut control_seen: HashSet<u64> = HashSet::new();
    let mut start_whole = whole;
    // Per step, collected and counted only if the step is one that replay would simulate.
    let mut step = [0u64; 8]; // ticks with a flip, control flips inside, control flips data reads, data flips, data flips control reads, ticks with such a flip, ticks whose control situation is known, ticks
    let mut total = [0u64; 8];
    let mut run = 0u64;
    while run < ticks && !engine.breakpoint_hit() {
        engine.step();
        run += 1;
        let state = engine.state();
        let (mut any, mut into_control, mut inputs) = (false, false, 0u64);
        let mut control_changed = 0u64;
        for e in 0..n {
            if state[e] == previous[e] {
                continue;
            }
            if live[e] {
                whole ^= z[e];
            }
            if !sim[e] {
                continue;
            }
            any = true;
            if data[e] {
                step[3] += 1;
                if read_by(e, false) {
                    step[4] += 1;
                    into_control = true;
                    inputs ^= z[e].rotate_left(17);
                }
            } else {
                control_state ^= z[e];
                control_changed ^= z[e].rotate_left(31);
                if read_by(e, true) {
                    step[2] += 1;
                } else {
                    step[1] += 1;
                }
            }
        }
        step[7] += 1;
        if any {
            step[0] += 1;
            step[5] += into_control as u64;
            // The control part's situation in this tick: its signals, what of it just changed, and which of its
            // inputs from data just flipped. Known if seen before: then its next state could be looked up.
            let situation = control_state ^ control_changed.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ inputs.wrapping_mul(0xbf58_476d_1ce4_e5b9);
            step[6] += !control_seen.insert(situation) as u64;
        }
        let (fetched, wrote) = engine.vmem_access();
        if fetched.is_some() || wrote.is_some() || engine.breakpoint_hit() {
            let word = fetched.map_or(0, |a| engine.vmem()[a]);
            if seen.insert((start_whole, word)) {
                for k in 0..8 {
                    total[k] += step[k];
                }
            }
            step = [0; 8];
            start_whole = whole;
        }
        previous.copy_from_slice(state);
    }
    let per = |x: u64| x as f64 / total[0].max(1) as f64;
    println!();
    println!("a fixed split from those contexts: {control_entities} control entities, {data_entities} data entities");
    println!("in the steps replay simulates, per tick that has a flip ({:.0}% of their ticks):", pct(total[0], total[7]));
    println!("    control flips nothing of data reads (would not be simulated): {:.2}", per(total[1]));
    println!("    control flips that data reads (would still be applied):         {:.2}", per(total[2]));
    println!("    data flips (still simulated):                                  {:.2}", per(total[3]));
    println!("    of those, data flips that control reads:                       {:.2}, in {:.0}% of these ticks (each such tick needs a hashed lookup)", per(total[4]), pct(total[5], total[0]));
    println!("    ticks in which the control part's situation was seen before:   {:.1}% ({} situations kept)", pct(total[6], total[0]), control_seen.len());
    Ok(())
}
