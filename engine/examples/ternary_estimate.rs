//! Idea A1 of the second research pass (specs/vcb-engine-v2.1-research2.md): how much of a step can be PROVEN
//! to be the same for any data, by simulating it with unknowns?
//!
//! A step runs from one VMem event to the next. Its context is what is known when it starts: the kind of event
//! before it and the address loaded there. Within a context, a signal that is not the same at the start of
//! every step is **unknown** (X) at the start. The unknowns are then followed through the step with the rules of
//! three-valued simulation: an AND with a known input that is off is known, whatever its other inputs are; an
//! OR with a known input that is on likewise; otherwise a gate with an unknown input is unknown; a latch is
//! unknown once it may or may not have been toggled. What stays known has the value seen here in every step of
//! the context that starts with the same known signals: an engine could apply those flips from a record. (The
//! reference engine supplies the values; only the known/unknown marks are computed here, which gives the same
//! answer as a three-valued simulation.)
//!
//! Counted over the steps replay misses today (every `sample`-th of them), flips of what v2 simulates.
//! The unknowns are taken from the steps of this run, so a longer run can only make more unknown.
//!
//!   cargo run --release --example ternary_estimate -- <board.vcb> <program.vcbasm or -> [ticks] [sample]
use std::collections::{HashMap, HashSet};
use std::path::Path;
use vcb::netlist::{Components, Kind};
use vcb::sim::Simulation;
use vcb::Ink;

fn mix(a: u64, b: u64) -> u64 {
    let x = (a ^ b).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    (x ^ x >> 29).wrapping_mul(0xbf58_476d_1ce4_e5b9).rotate_left(27) ^ b
}

/// Per context: the first step's signals at its start and one tick earlier, and which signals differed in a later step.
#[derive(Default)]
struct Context {
    steps: u32,
    /// Steps (after the first) that began with a signal different that had been the same in all before: each
    /// would fail the check of a record made so far and force a new proof.
    grew: u32,
    /// The lengths of its steps.
    lengths: HashSet<u64>,
    first: [Vec<bool>; 2],
    unknown: [Vec<bool>; 2],
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let mut board = vcb::Board::read(Path::new(&args[1]))?;
    let source = if args[2] == "-" { board.assembly.clone() } else { std::fs::read_to_string(&args[2]).map_err(|e| e.to_string())?.replace("\r\n", "\n") };
    vcb::asm::assemble(&source, &args[2], &mut board, true)?;
    let ticks: u64 = args.get(3).map_or(5_000_000, |t| t.parse().unwrap());
    let sample: usize = args.get(4).map_or(5, |t| t.parse().unwrap());
    let options = vcb::sim::Options { timer_ticks: Some(1000) };
    let c = Components::build(&board)?;
    let n = c.kinds.len();
    // What matters: everything that can reach a VMem pin, a Breakpoint or a Random (the clock divider included).
    let mut sim = vec![false; n];
    let mut todo: Vec<usize> = (0..n).filter(|e| matches!(c.kinds[*e], Kind::Pin | Kind::Component(Ink::Breakpoint | Ink::Random))).collect();
    while let Some(e) = todo.pop() {
        if !std::mem::replace(&mut sim[e], true) {
            todo.extend((if c.kinds[e] == Kind::Trace { &c.writers[e] } else { &c.reads[e] }).iter().map(|x| *x as usize));
        }
    }
    let list: Vec<usize> = (0..n).filter(|e| sim[*e]).collect();
    let (components, traces): (Vec<usize>, Vec<usize>) = list.iter().partition(|e| c.kinds[**e] != Kind::Trace);
    // Whose flips are counted: what v2 evaluates (not what it flips from the clock schedule).
    let fast = vcb::sim2::Engine::new(&board, c.clone(), &options)?;
    let scheduled = fast.scheduled();
    let counted: Vec<bool> = (0..n).map(|e| fast.live()[e] && !scheduled[e]).collect();
    let readers = |y: usize| if c.kinds[y] == Kind::Trace { &c.readers[y] } else { &c.writes[y] };
    let mut seed = 0x1234_5678_9abc_def1u64;
    let z: Vec<u64> = (0..n).map(|_| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    }).collect();

    // First run: the steps, their contexts, whether replay knows them, and what is unknown at the start of a context.
    let mut contexts: HashMap<(u8, u32), Context> = HashMap::new();
    let mut steps: Vec<((u8, u32), bool)> = Vec::new();
    let mut lengths: Vec<((u8, u32), u64)> = Vec::new();
    {
        let mut engine = vcb::sim::Engine::new(&board, c.clone(), &options)?;
        let mut previous = engine.state().to_vec();
        let mut whole: u64 = list.iter().filter(|e| previous[**e]).fold(0, |h, e| h ^ z[*e]);
        let (mut start_whole, mut start_extra, mut context, mut start_tick) = (whole, 0u64, (0u8, u32::MAX), 0u64);
        let mut seen: HashSet<u64> = HashSet::new();
        let mut note = |context: (u8, u32), before: &[bool], now: &[bool]| {
            let entry = contexts.entry(context).or_default();
            entry.steps += 1;
            for (k, state) in [now, before].into_iter().enumerate() {
                if entry.steps == 1 {
                    entry.first[k] = state.to_vec();
                    entry.unknown[k] = vec![false; n];
                } else {
                    let mut grew = false;
                    for &e in &list {
                        grew |= state[e] != entry.first[k][e] && !entry.unknown[k][e];
                        entry.unknown[k][e] |= state[e] != entry.first[k][e];
                    }
                    entry.grew += (grew && k == 0) as u32;
                }
            }
        };
        note(context, &previous, &previous);
        let mut run = 0u64;
        while run < ticks && !engine.breakpoint_hit() {
            engine.step();
            run += 1;
            let state = engine.state();
            let mut extra = 0u64;
            for &e in &list {
                if state[e] != previous[e] {
                    whole ^= z[e];
                    extra ^= mix(z[e], 99);
                }
            }
            let (fetched, wrote) = engine.vmem_access();
            if fetched.is_some() || wrote.is_some() || engine.breakpoint_hit() {
                let word = fetched.map_or(0, |a| engine.vmem()[a]) as u64;
                steps.push((context, !seen.insert(mix(mix(start_whole, start_extra), word))));
                lengths.push((context, engine.tick() - start_tick));
                start_tick = engine.tick();
                start_whole = whole;
                start_extra = extra ^ (fetched.is_some() as u64 * 2 + wrote.is_some() as u64 + 7);
                context = (fetched.is_some() as u8 * 2 + wrote.is_some() as u8, fetched.map_or(u32::MAX, |a| a as u32));
                note(context, &previous, state);
            }
            previous.copy_from_slice(state);
        }
    }

    for (context, length) in lengths {
        contexts.get_mut(&context).unwrap().lengths.insert(length);
    }
    // Second run: follow the unknowns through the sampled steps.
    let mut engine = vcb::sim::Engine::new(&board, c.clone(), &options)?;
    let mut previous = engine.state().to_vec();
    let mut context = (0u8, u32::MAX);
    // Unknown now, one tick ago, and in the tick being computed.
    let (mut unknown, mut earlier, mut next) = (contexts[&context].unknown[0].clone(), contexts[&context].unknown[1].clone(), vec![false; n]);
    let mut index = 0usize;
    let mut misses = 0usize;
    let follow = |index: usize, misses: usize| !steps[index].1 && contexts[&steps[index].0].steps > 1 && misses % sample == 0;
    let mut start = previous.clone();
    // Over the followed steps: steps, flips, flips of a signal known before and after, the unknown readers such
    // flips are told to, unknown signals at the start and the end, known signals that differ at the end,
    // steps in which no address pin became unknown, ticks.
    let mut total = [0u64; 14];
    // In the step being followed: which signals were unknown at some tick, and every counted flip.
    let mut ever = vec![false; n];
    let mut step_flips: Vec<usize> = Vec::new();
    ever.copy_from_slice(&unknown);
    let mut address_known = true;
    let mut run = 0u64;
    while run < ticks && !engine.breakpoint_hit() && index < steps.len() {
        engine.step();
        run += 1;
        let state = engine.state();
        let following = follow(index, misses);
        if following {
            for &e in &components {
                let inputs = &c.reads[e];
                let known_input = |v: bool| inputs.iter().any(|t| !unknown[*t as usize] && previous[*t as usize] == v);
                let any_unknown = || inputs.iter().any(|t| unknown[*t as usize]);
                next[e] = match c.kinds[e] {
                    Kind::Component(Ink::Clock | Ink::Timer) => false,
                    Kind::Component(Ink::And | Ink::Nand) => !known_input(false) && any_unknown(),
                    Kind::Component(Ink::Buffer | Ink::Or | Ink::Nor | Ink::Not | Ink::Led | Ink::Breakpoint | Ink::Wireless(_)) => !known_input(true) && any_unknown(),
                    // A latch (a pin is one): unknown once an input may or may not have risen in the tick before.
                    Kind::Pin | Kind::Component(Ink::LatchOn | Ink::LatchOff) => unknown[e] || inputs.iter().any(|t| unknown[*t as usize] || (previous[*t as usize] && earlier[*t as usize])),
                    _ => any_unknown(),
                };
            }
            // A trace is on when a writer is: known if a known writer is on, or all its writers are known.
            for &t in &traces {
                let writers = &c.writers[t];
                next[t] = !writers.iter().any(|w| !next[*w as usize] && state[*w as usize]) && writers.iter().any(|w| next[*w as usize]);
            }
            for &e in &list {
                ever[e] |= next[e];
                if counted[e] && state[e] != previous[e] {
                    total[1] += 1;
                    step_flips.push(e);
                    if !unknown[e] && !next[e] {
                        total[2] += 1;
                        total[3] += readers(e).iter().filter(|r| sim[**r as usize] && next[**r as usize]).count() as u64;
                    }
                }
            }
            address_known &= c.address_pins.iter().all(|p| !next[*p as usize]);
            total[8] += 1;
            std::mem::swap(&mut earlier, &mut unknown);
            std::mem::swap(&mut unknown, &mut next);
        }
        let (fetched, wrote) = engine.vmem_access();
        if fetched.is_some() || wrote.is_some() || engine.breakpoint_hit() {
            if following {
                total[0] += 1;
                total[4] += list.iter().filter(|e| counted[**e] && contexts[&context].unknown[0][**e]).count() as u64;
                total[5] += list.iter().filter(|e| counted[**e] && unknown[**e]).count() as u64;
                total[6] += list.iter().filter(|e| counted[**e] && !unknown[**e] && state[**e] != start[**e]).count() as u64;
                total[7] += address_known as u64;
                // The split an engine could use: a signal unknown at any tick of the step is simulated for the
                // whole step (the variable part); the rest is applied from the record (the fixed part).
                total[9] += list.iter().filter(|e| counted[**e] && ever[**e]).count() as u64;
                for &e in &step_flips {
                    if ever[e] {
                        total[10] += 1;
                    } else {
                        total[11] += readers(e).iter().filter(|r| sim[**r as usize] && ever[**r as usize]).count() as u64;
                    }
                }
                for &e in list.iter().filter(|e| counted[**e] && !ever[**e] && state[**e] != start[**e]) {
                    total[12] += 1;
                    total[13] += readers(e).iter().filter(|r| sim[**r as usize]).count() as u64;
                }
            }
            step_flips.clear();
            ever.copy_from_slice(&contexts[&(fetched.is_some() as u8 * 2 + wrote.is_some() as u8, fetched.map_or(u32::MAX, |a| a as u32))].unknown[0]);
            misses += (!steps[index].1 && contexts[&steps[index].0].steps > 1) as usize;
            index += 1;
            context = (fetched.is_some() as u8 * 2 + wrote.is_some() as u8, fetched.map_or(u32::MAX, |a| a as u32));
            unknown.copy_from_slice(&contexts[&context].unknown[0]);
            earlier.copy_from_slice(&contexts[&context].unknown[1]);
            start.copy_from_slice(state);
            address_known = true;
        }
        previous.copy_from_slice(state);
    }
    let missed = steps.iter().filter(|s| !s.1).count();
    let alone = steps.iter().filter(|s| !s.1 && contexts[&s.0].steps == 1).count();
    let per = |x: u64| x as f64 / total[0].max(1) as f64;
    println!("{run} ticks, {} steps in {} contexts; replay today misses {missed} ({alone} of them the only step of their context); followed: {}", steps.len(), contexts.len(), total[0]);
    println!("per followed step ({:.0} ticks), of the {} signals v2 evaluates:", per(total[8]), counted.iter().filter(|x| **x).count());
    println!("    unknown at the start {:.0}, unknown at the end {:.0}", per(total[4]), per(total[5]));
    println!("    flips {:.0}; proven the same for any data {:.0} ({:.1}%)", per(total[1]), per(total[2]), 100.0 * total[2] as f64 / total[1].max(1) as f64);
    println!("    times such a proven flip is told to a reader that is unknown: {:.0}", per(total[3]));
    println!("    known signals that differ between start and end: {:.0}", per(total[6]));
    println!("    steps in which every address pin stays known: {:.1}%", 100.0 * per(total[7]));
    println!("the split an engine could use (a signal unknown at any tick of the step is simulated all through it):");
    println!("    variable part: {:.0} signals, {:.0} flips ({:.1}% of the step's flips are left to simulate)", per(total[9]), per(total[10]), 100.0 * total[10] as f64 / total[1].max(1) as f64);
    println!("    flips of the fixed part told to the variable part, at their tick: {:.0}", per(total[11]));
    println!("    at the end: {:.0} fixed signals differ from the start, with {:.0} readers to adjust", per(total[12]), per(total[13]));
    let all: u32 = contexts.values().map(|c| c.steps).sum();
    let grew: u32 = contexts.values().map(|c| c.grew).sum();
    let uneven: u32 = contexts.values().filter(|c| c.lengths.len() > 1).map(|c| c.steps).sum();
    println!("steps that began with a signal changed that had been the same in all steps of the context before (a record would fail and be proven anew): {:.1}% of all steps", 100.0 * grew as f64 / all as f64);
    println!("steps in a context whose steps are not all of one length: {:.1}%", 100.0 * uneven as f64 / all as f64);
    Ok(())
}
