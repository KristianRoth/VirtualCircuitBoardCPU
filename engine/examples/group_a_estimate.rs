//! Estimates for group A of the second research pass (specs/vcb-engine-v2.1-research2.md), on the reference engine.
//! A step runs from one VMem event to the next, as in v2's replay. Counted, by ticks inside steps:
//!
//! - **Today**: steps whose whole start state was seen before.
//! - **Read set**: steps whose own read set, with its values, was seen before (the limit; step 1 of v2.1).
//! - **Mask per context (A3)**: a context is what is known when a step starts: the kind of event before it and
//!   the address loaded there. Its mask is every signal that any step of the context read. A step is a hit if
//!   its start state under that mask was seen before in the context. Unlike the read set this is one hashed
//!   lookup, since the mask is known before the step runs. The mask used is the final one of the whole run, so
//!   the figure is a little too good. "Sharp" masks count only the inputs that decide a gate (see readset_estimate).
//! - **The net change (measurement N)**: signals that differ between a step's start and end, against its flips.
//! - **Inside a step (A7)**: when a step that replay misses comes to rest (a tick in which nothing changes) in
//!   a whole state seen at such a tick before, the ticks after it could be replayed. Too good as well: it
//!   leaves out the clock's position and the word that the step loads.
//!
//!   cargo run --release --example group_a_estimate -- <board.vcb> <program.vcbasm or -> [ticks]
use std::collections::{HashMap, HashSet};
use std::path::Path;
use vcb::netlist::{Components, Kind};
use vcb::sim::Simulation;
use vcb::Ink;

fn mix(a: u64, b: u64) -> u64 {
    let x = (a ^ b).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    (x ^ x >> 29).wrapping_mul(0xbf58_476d_1ce4_e5b9).rotate_left(27) ^ b
}

struct Step {
    context: (u8, u32),
    start: Vec<u64>,
    extra: u64,
    word: u64,
    length: u64,
    known: bool,
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let mut board = vcb::Board::read(Path::new(&args[1]))?;
    let source = if args[2] == "-" { board.assembly.clone() } else { std::fs::read_to_string(&args[2]).map_err(|e| e.to_string())?.replace("\r\n", "\n") };
    vcb::asm::assemble(&source, &args[2], &mut board, true)?;
    let ticks: u64 = args.get(3).map_or(5_000_000, |t| t.parse().unwrap());
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
    // What v2 flips from its clock schedule, and the traces only that writes: it never rests, so "at rest" leaves it out.
    let fast = vcb::sim2::Engine::new(&board, c.clone(), &vcb::sim::Options { timer_ticks: Some(1000) })?;
    let scheduled = fast.scheduled();
    let merged = |e: usize| c.kinds[e] == Kind::Trace && c.writers[e].len() == 1 && fast.live()[c.writers[e][0] as usize] && !scheduled[c.writers[e][0] as usize];
    let clocked: Vec<bool> = (0..n).map(|e| scheduled[e] || !(fast.live()[e] || merged(e)) || (c.kinds[e] == Kind::Trace && !c.writers[e].is_empty() && c.writers[e].iter().all(|w| scheduled[*w as usize]))).collect();
    let mut resting = true;
    let mut slot = vec![usize::MAX; n];
    list.iter().enumerate().for_each(|(i, e)| slot[*e] = i);
    let words = list.len().div_ceil(64);
    let inputs = |r: usize| if c.kinds[r] == Kind::Trace { &c.writers[r] } else { &c.reads[r] };
    let readers = |y: usize| if c.kinds[y] == Kind::Trace { &c.readers[y] } else { &c.writes[y] };
    let decided_by = |r: usize| match c.kinds[r] {
        Kind::Trace => Some(true),
        Kind::Component(Ink::And | Ink::Nand) => Some(false),
        Kind::Component(Ink::Buffer | Ink::Or | Ink::Nor | Ink::Not | Ink::Led | Ink::Breakpoint | Ink::Wireless(_)) => Some(true),
        _ => None,
    };
    let mut seed = 0x1234_5678_9abc_def1u64;
    let z: Vec<u64> = (0..n).map(|_| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    }).collect();

    let mut engine = vcb::sim::Engine::new(&board, c.clone(), &vcb::sim::Options { timer_ticks: Some(1000) })?;
    let mut previous = engine.state().to_vec();
    let pack = |state: &[bool]| {
        let mut bits = vec![0u64; words];
        list.iter().enumerate().filter(|(_, e)| state[**e]).for_each(|(i, _)| bits[i >> 6] |= 1 << (i & 63));
        bits
    };
    let mut whole: u64 = list.iter().filter(|e| previous[**e]).fold(0, |h, e| h ^ z[*e]);
    let (mut read_stamp, mut sharp_stamp, mut write_stamp) = (vec![0u32; n], vec![0u32; n], vec![0u32; n]);
    let mut number = 1u32;
    let (mut changed, mut before): (Vec<usize>, Vec<usize>) = (Vec::new(), Vec::new());
    let (mut read_hash, mut sharp_hash) = (0u64, 0u64);
    let (mut read_set, mut sharp_set): (Vec<usize>, Vec<usize>) = (Vec::new(), Vec::new());
    let (mut start_tick, mut start_whole, mut start_extra, mut start_bits, mut context) = (0u64, whole, 0u64, pack(&previous), (0u8, u32::MAX));
    let (mut seen_whole, mut seen_read, mut seen_sharp, mut seen_rest): (HashSet<u64>, HashSet<u64>, HashSet<u64>, HashSet<u64>) = Default::default();
    // Per context: the signals any of its steps read, as bit masks over `list` (all reads, and the sharper ones).
    let mut masks: HashMap<(u8, u32), (Vec<u64>, Vec<u64>)> = HashMap::new();
    let mut steps: Vec<Step> = Vec::new();
    let (mut ticks_read, mut ticks_sharp) = (0u64, 0u64);
    // Over the steps replay misses: flips, signals flipped, signals that differ at the end; ticks after a known rest.
    let (mut missed, mut flips, mut missed_flips, mut missed_written, mut missed_net, mut after_rest) = (0u64, 0u64, 0u64, 0u64, 0u64, 0u64);
    let mut written = 0u64;
    let mut rest_hit: Option<u64> = None;
    let mut run = 0u64;
    while run < ticks && !engine.breakpoint_hit() {
        engine.step();
        run += 1;
        let state = engine.state();
        for &y in &before {
            for &r in readers(y) {
                let r = r as usize;
                if !sim[r] {
                    continue;
                }
                for &x in inputs(r).iter().chain(std::iter::once(&(r as u32))) {
                    let x = x as usize;
                    if sim[x] && write_stamp[x] != number && read_stamp[x] != number {
                        read_stamp[x] = number;
                        read_hash ^= mix(z[x], previous[x] as u64 + 1);
                        read_set.push(x);
                    }
                }
                let mut read = |x: usize| {
                    if sim[x] && write_stamp[x] != number && sharp_stamp[x] != number {
                        sharp_stamp[x] = number;
                        sharp_hash ^= mix(z[x], previous[x] as u64 + 1);
                        sharp_set.push(x);
                    }
                };
                read(r);
                if !matches!(c.kinds[r], Kind::Pin | Kind::Component(Ink::LatchOn | Ink::LatchOff | Ink::Clock | Ink::Timer)) {
                    let deciding = decided_by(r).and_then(|v| {
                        let with_value = |x: &&u32| previous[**x as usize] == v;
                        inputs(r).iter().filter(with_value).find(|x| write_stamp[**x as usize] == number).or_else(|| inputs(r).iter().find(with_value))
                    });
                    match deciding {
                        Some(x) => read(*x as usize),
                        None => inputs(r).iter().for_each(|x| read(*x as usize)),
                    }
                }
            }
        }
        changed.clear();
        changed.extend(list.iter().copied().filter(|e| state[*e] != previous[*e]));
        flips += changed.len() as u64;
        for &e in &changed {
            if write_stamp[e] != number {
                write_stamp[e] = number;
                written += 1;
            }
            whole ^= z[e];
        }
        // The circuit has come to rest inside the step: has it rested in this state before?
        let quiet = changed.iter().all(|e| clocked[*e]);
        if quiet && !resting && !seen_rest.insert(whole) && rest_hit.is_none() {
            rest_hit = Some(engine.tick());
        }
        resting = quiet;
        let (fetched, wrote) = engine.vmem_access();
        if fetched.is_some() || wrote.is_some() || engine.breakpoint_hit() {
            let word = fetched.map_or(0, |a| engine.vmem()[a]) as u64;
            let length = engine.tick() - start_tick;
            let known = !seen_whole.insert(mix(mix(start_whole, start_extra), word));
            ticks_read += if seen_read.insert(mix(mix(read_hash, start_extra), word)) { 0 } else { length };
            ticks_sharp += if seen_sharp.insert(mix(mix(sharp_hash, start_extra), word)) { 0 } else { length };
            let end_bits = pack(state);
            if !known {
                missed += 1;
                missed_flips += flips;
                missed_written += written;
                missed_net += start_bits.iter().zip(&end_bits).map(|(a, b)| (a ^ b).count_ones() as u64).sum::<u64>();
                after_rest += rest_hit.map_or(0, |t| engine.tick() - t);
            }
            let mask = masks.entry(context).or_insert_with(|| (vec![0; words], vec![0; words]));
            read_set.drain(..).for_each(|x| mask.0[slot[x] >> 6] |= 1 << (slot[x] & 63));
            sharp_set.drain(..).for_each(|x| mask.1[slot[x] >> 6] |= 1 << (slot[x] & 63));
            steps.push(Step { context, start: std::mem::replace(&mut start_bits, end_bits), extra: start_extra, word, length, known });
            number += 1;
            (read_hash, sharp_hash, flips, written, rest_hit) = (0, 0, 0, 0, None);
            start_tick = engine.tick();
            start_whole = whole;
            start_extra = changed.iter().fold(fetched.is_some() as u64 * 2 + wrote.is_some() as u64 + 7, |h, e| h ^ mix(z[*e], 99));
            context = (fetched.is_some() as u8 * 2 + wrote.is_some() as u8, fetched.map_or(u32::MAX, |a| a as u32));
        }
        std::mem::swap(&mut before, &mut changed);
        previous.copy_from_slice(state);
    }

    // The masks are complete: which steps start in a state seen before under their context's mask?
    let mut hits = [0u64; 2];
    let mut seen: [HashSet<u64>; 2] = Default::default();
    for step in &steps {
        let mask = &masks[&step.context];
        for (k, mask) in [&mask.0, &mask.1].into_iter().enumerate() {
            let state = step.start.iter().zip(mask).fold(0u64, |h, (s, m)| mix(h, s & m));
            let key = mix(mix(mix(state, step.extra), step.word), mix(step.context.0 as u64, step.context.1 as u64));
            hits[k] += if seen[k].insert(key) { 0 } else { step.length };
        }
    }
    let total: u64 = steps.iter().map(|s| s.length).sum();
    let today: u64 = steps.iter().filter(|s| s.known).map(|s| s.length).sum();
    let pct = |x: u64| 100.0 * x as f64 / total as f64;
    let size = |pick: fn(&(Vec<u64>, Vec<u64>)) -> &Vec<u64>| steps.iter().map(|s| pick(&masks[&s.context]).iter().map(|w| w.count_ones() as u64).sum::<u64>()).sum::<u64>() as f64 / steps.len() as f64;
    println!("{} signals that matter; {total} ticks in {} steps, {} contexts (the event before a step and its address)", list.len(), steps.len(), masks.len());
    println!("ticks inside steps that would be replayed:");
    println!("    whole start state seen before (today):                      {:.1}%", pct(today));
    println!("    start state under the context's mask seen before (A3):      {:.1}%   (mask: {:.0} signals on average)", pct(hits[0]), size(|m| &m.0));
    println!("    the same with the sharper mask:                             {:.1}%   (mask: {:.0} signals)", pct(hits[1]), size(|m| &m.1));
    println!("    the step's own read set seen before (limit):                {:.1}%", pct(ticks_read));
    println!("    the step's own sharper read set seen before (limit):        {:.1}%", pct(ticks_sharp));
    let per = |x: u64| x as f64 / missed.max(1) as f64;
    println!("in the {missed} steps replay misses today: {:.0} flips per step, {:.0} signals flipped, {:.0} signals differ at the end (the net change)", per(missed_flips), per(missed_written), per(missed_net));
    println!("ticks of those steps that come after a rest in a state seen at rest before (A7): {:.1}% of all ticks, {:.1}% of the missed ones", pct(after_rest), 100.0 * after_rest as f64 / (total - today).max(1) as f64);
    println!("    (different states at rest: {})", seen_rest.len());
    Ok(())
}
