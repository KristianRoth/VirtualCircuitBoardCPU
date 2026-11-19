//! Step 1 of v2.1 (specs/vcb-engine-v2.1.md): what would replay gain if a step were reused whenever the state
//! agrees on what the step actually READ, and not only when the whole state is the same?
//!
//! A step runs from one VMem event to the next, as in v2's replay. While the reference engine simulates it,
//! this tool collects the step's **read set**: every signal whose value from before the step some gate looked
//! at during it. A gate looks at all its inputs and its own state whenever one of its inputs changed. A signal
//! that the step itself changed first is not read (its value came from other signals that were).
//! The rule is sound: the circuit is deterministic, so a step does the same in any state that agrees on its
//! read set. Counted: the share of ticks inside steps whose whole start state was seen before (v2 today),
//! and inside steps whose read set, with its values, was seen before (the upper limit of the new scheme: it
//! says a matching step exists, not what finding it costs).
//!
//! A second, sharper read set uses what decides a gate: an AND with an input off is off whatever its other
//! inputs are, so only that one input is read (and the gate's own state); an OR with an input on likewise; a
//! latch reads only itself (it reacts to changes, and a change is something the step made). That is how
//! control hides data: a data bit behind a closed enable is not read.
//!
//!   cargo run --release --example readset_estimate -- <board.vcb> <program.vcbasm or -> [ticks]
use std::collections::HashSet;
use std::path::Path;
use vcb::netlist::{Components, Kind};
use vcb::sim::Simulation;
use vcb::Ink;

fn mix(a: u64, b: u64) -> u64 {
    let x = (a ^ b).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    (x ^ x >> 29).wrapping_mul(0xbf58_476d_1ce4_e5b9).rotate_left(27) ^ b
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let mut board = vcb::Board::read(Path::new(&args[1]))?;
    let source = if args[2] == "-" { board.assembly.clone() } else { std::fs::read_to_string(&args[2]).map_err(|e| e.to_string())?.replace("\r\n", "\n") };
    vcb::asm::assemble(&source, &args[2], &mut board, true)?;
    let ticks: u64 = args.get(3).map_or(20_000_000, |t| t.parse().unwrap());
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
    let inputs = |r: usize| if c.kinds[r] == Kind::Trace { &c.writers[r] } else { &c.reads[r] };
    let readers = |y: usize| if c.kinds[y] == Kind::Trace { &c.readers[y] } else { &c.writes[y] };
    let mut seed = 0x1234_5678_9abc_def1u64;
    let z: Vec<u64> = (0..n).map(|_| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    }).collect();

    let mut engine = vcb::sim::Engine::new(&board, c.clone(), &vcb::sim::Options { timer_ticks: Some(1000) })?;
    let mut previous = engine.state().to_vec();
    let mut whole: u64 = (0..n).filter(|e| sim[*e] && previous[*e]).fold(0, |h, e| h ^ z[e]);
    let (mut read_stamp, mut write_stamp) = (vec![0u32; n], vec![0u32; n]);
    let mut sharp_stamp = vec![0u32; n];
    let (mut sharp_hash, mut sharp_size) = (0u64, 0usize);
    let mut seen_sharp: HashSet<u64> = HashSet::new();
    let (mut ticks_sharp, mut sum_sharp) = (0u64, 0u64);
    let mut sharp_sizes: Vec<u32> = Vec::new();
    // How a kind of entity is decided: Some(v): one input with value v decides it; None: every input counts.
    let decided_by = |r: usize| match c.kinds[r] {
        Kind::Trace => Some(true),
        Kind::Component(Ink::And | Ink::Nand) => Some(false),
        Kind::Component(Ink::Buffer | Ink::Or | Ink::Nor | Ink::Not | Ink::Led | Ink::Breakpoint | Ink::Wireless(_)) => Some(true),
        _ => None,
    };
    let mut step = 1u32;
    let (mut changed, mut before): (Vec<usize>, Vec<usize>) = (Vec::new(), Vec::new());
    // The current step: its read set as a hash and a size, what it wrote, where it began.
    let (mut read_hash, mut read_size, mut written) = (0u64, 0usize, 0usize);
    let (mut start_tick, mut start_whole, mut start_extra) = (0u64, whole, 0u64);
    let (mut seen_whole, mut seen_read): (HashSet<u64>, HashSet<u64>) = (HashSet::new(), HashSet::new());
    let (mut steps, mut ticks_whole, mut ticks_read, mut sum_read, mut sum_written) = (0u64, 0u64, 0u64, 0u64, 0u64);
    let mut sizes: Vec<u32> = Vec::new();
    let mut run = 0u64;
    while run < ticks && !engine.breakpoint_hit() {
        engine.step();
        run += 1;
        let state = engine.state();
        // What this tick's evaluations looked at: for everything that changed in the tick before, each reader's
        // inputs and the reader itself, as they were before this tick, unless the step already changed them.
        for &y in &before {
            for &r in readers(y) {
                let r = r as usize;
                if !sim[r] {
                    continue;
                }
                for &x in inputs(r).iter().chain(std::iter::once(&(r as u32))) {
                    let x = x as usize;
                    if sim[x] && write_stamp[x] != step && read_stamp[x] != step {
                        read_stamp[x] = step;
                        read_hash ^= mix(z[x], previous[x] as u64 + 1);
                        read_size += 1;
                    }
                }
                // The sharper set: the reader itself, and only the inputs that decide it.
                let mut read = |x: usize| {
                    if sim[x] && write_stamp[x] != step && sharp_stamp[x] != step {
                        sharp_stamp[x] = step;
                        sharp_hash ^= mix(z[x], previous[x] as u64 + 1);
                        sharp_size += 1;
                    }
                };
                read(r);
                let latch = matches!(c.kinds[r], Kind::Pin | Kind::Component(Ink::LatchOn | Ink::LatchOff | Ink::Clock | Ink::Timer));
                if !latch {
                    let deciding = decided_by(r).and_then(|v| {
                        // An input the step already knows about is the cheapest proof; otherwise the first one.
                        let with_value = |x: &&u32| previous[**x as usize] == v;
                        inputs(r).iter().filter(with_value).find(|x| write_stamp[**x as usize] == step).or_else(|| inputs(r).iter().find(with_value))
                    });
                    match deciding {
                        Some(x) => read(*x as usize),
                        None => inputs(r).iter().for_each(|x| read(*x as usize)),
                    }
                }
            }
        }
        changed.clear();
        changed.extend((0..n).filter(|e| sim[*e] && state[*e] != previous[*e]));
        for &e in &changed {
            if write_stamp[e] != step {
                write_stamp[e] = step;
                written += 1;
            }
            whole ^= z[e];
        }
        let (fetched, wrote) = engine.vmem_access();
        if fetched.is_some() || wrote.is_some() || engine.breakpoint_hit() {
            // The step ends. What follows also depends on the word just loaded, as in v2's replay.
            let word = fetched.map_or(0, |a| engine.vmem()[a]) as u64;
            let length = engine.tick() - start_tick;
            let key_whole = mix(mix(start_whole, start_extra), word);
            let key_read = mix(mix(read_hash, start_extra), word);
            steps += 1;
            ticks_whole += if seen_whole.insert(key_whole) { 0 } else { length };
            ticks_read += if seen_read.insert(key_read) { 0 } else { length };
            let key_sharp = mix(mix(sharp_hash, start_extra), word);
            ticks_sharp += if seen_sharp.insert(key_sharp) { 0 } else { length };
            sum_sharp += sharp_size as u64;
            sharp_sizes.push(sharp_size as u32);
            (sharp_hash, sharp_size) = (0, 0);
            sum_read += read_size as u64;
            sum_written += written as u64;
            sizes.push(read_size as u32);
            step += 1;
            (read_hash, read_size, written) = (0, 0, 0);
            start_tick = engine.tick();
            start_whole = whole;
            // What is pending at the start of the next step: what changed in this last tick, and what VMem just did.
            start_extra = changed.iter().fold(fetched.is_some() as u64 * 2 + wrote.is_some() as u64 + 7, |h, e| h ^ mix(z[*e], 99));
        }
        std::mem::swap(&mut before, &mut changed);
        previous.copy_from_slice(state);
    }
    sizes.sort();
    let simulated = sim.iter().filter(|s| **s).count();
    let pct = |x: u64| 100.0 * x as f64 / run as f64;
    println!("{simulated} signals that matter; {run} ticks in {steps} steps of {:.0} ticks", run as f64 / steps as f64);
    println!("ticks inside steps whose whole start state was seen before (replay today): {:.1}%", pct(ticks_whole));
    println!("ticks inside steps whose read set was seen before (the limit of read-set replay): {:.1}%", pct(ticks_read));
    println!("ticks inside steps whose sharper read set was seen before (only the inputs that decide a gate): {:.1}%", pct(ticks_sharp));
    sharp_sizes.sort();
    println!("different whole states {}, different read sets {}, different sharper read sets {}", seen_whole.len(), seen_read.len(), seen_sharp.len());
    println!("the sharper read set has {:.0} signals on average (median {}, 90th percentile {})", sum_sharp as f64 / steps as f64, sharp_sizes[sharp_sizes.len() / 2], sharp_sizes[sharp_sizes.len() * 9 / 10]);
    println!("a step reads {:.0} signals on average (median {}, 90th percentile {}, most {}) of {simulated}, and changes {:.0}", sum_read as f64 / steps as f64, sizes[sizes.len() / 2], sizes[sizes.len() * 9 / 10], sizes[sizes.len() - 1], sum_written as f64 / steps as f64);
    Ok(())
}
