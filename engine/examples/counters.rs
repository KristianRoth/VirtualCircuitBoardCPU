//! What the processor's own counters say about a run of v2: cycles, instructions, mispredicted branches and
//! cache misses, per kind of step. macOS on Apple Silicon only, and it needs root (the counters are read through
//! Apple's private kperf interface, by the `darwin-kperf` crate):
//!
//!   cargo build --release --example counters
//!   sudo target/release/examples/counters <board.vcb> <program.vcbasm or -> [ticks] [every] [nosplit] [table=<MiB>] [events=branches] [parts] [dry]
//!
//! The counters are read once after every call of `advance`, that is once per step (a step runs from one VMem
//! event to the next), and what was counted in between goes to the kind of step it was: taken from the replay
//! table, simulated with a record (only the variable part), or simulated whole. They count this thread only,
//! so the worker thread's proofs are not in them. Reading the counters costs something itself; that is measured
//! on empty stretches first and taken off, once per reading.
//!
//! A reading goes through the kernel, which pushes the program's data out of the first cache and disturbs the
//! branch predictor: the step after a reading then misses more than it would have. `every` (1 unless given)
//! reads only after every so many steps; the steps in between cannot be told apart then, so everything is
//! reported as one kind. The difference between `every` 1 and, say, 1000 is what the readings themselves cause.
//!
//! `events=branches` counts another set of events: the mispredicted branches by kind (conditional, indirect,
//! returns, indirect calls) and the conditional and indirect branches executed, for a rate per branch. The
//! processor cannot count every combination at once, and which it can is not written down anywhere, so the tool
//! asks for the full set and then for smaller ones, and says which it got. (`events=caches` is the usual set.)
//!
//! `parts` reads the counters inside a step too, where a part of it ends, and reports per kind of step and per
//! part. It needs an engine built with the calls at those places, in a target directory of its own:
//!
//!   RUSTFLAGS="--cfg parts -C target-cpu=native" cargo build --release --example counters --target-dir target/parts
//!   sudo target/parts/release/examples/counters <board.vcb> <program.vcbasm or -> [ticks] 1 parts
//!
//! `dry` needs no root and measures nothing: the clock's nanoseconds stand in for every event. It is there to try
//! the tool itself (the words, the boundaries, the report) before a run with sudo.
//!
//! What it cannot tell:
//! - With `parts` a step with a record is read four or five times instead of once. Every reading's cycles and
//!   instructions are taken off, but each one also empties part of the first cache and the TLB for what follows
//!   it, so the load, store and TLB misses of the parts are higher than in an undisturbed run, and more so than
//!   with one reading per step (which already gave load misses a sixth too high and TLB misses twice too high).
//!   Instructions and mispredicted branches are not affected. The cycles are, as far as misses cost cycles.
//! - Which function inside a part the instructions are in: a part is all that runs between two boundaries.
//! - The parts of a step that ends at the tick limit instead of an event (the last one of a run, at most).
//! - Whether the processor's count of indirect branches executed includes returns; its count of mispredicted
//!   indirect branches does. The rate printed for them takes the returns off when they were counted.
use darwin_kperf::{event::Event, Sampler};
use std::cell::RefCell;
use std::path::Path;
use vcb::netlist::Components;
use vcb::sim::Simulation;

const KINDS: [&str; 5] = ["taken from the replay table", "simulated with a record", "simulated whole", "other", "all steps together"];
/// The parts of a step, in the engine's numbering (`PART_HOOK` in src/sim2.rs); the last one ends at the tool's own reading.
const PARTS: [(&str, &str); 5] = [
    ("restore", "from the start of `advance` to the end of `restore`: the replay lookup that failed and putting the state into the circuit (only when replay had run ahead)"),
    ("begin_step", "to the end of `begin_step`: finding the context and the record (with the failed replay lookup, when there was no `restore`)"),
    ("ticks", "the step's ticks: to where `leave_split` starts, or to the end of the tick with the event"),
    ("leave_split", "`leave_split`: making the circuit whole after a step with a record"),
    ("end", "after the last tick to the return of `advance`: `intern` and the replay table's recording. A replayed step is all here, and so is everything without `parts`"),
];
const END: usize = 4;
/// The most events counted at once (two fixed counters and seven others).
const MAX: usize = 9;
type Reading = [u64; MAX];

/// The counters, and what they counted in each part of the step that is running.
struct Meter {
    read: Box<dyn Fn() -> Reading>,
    last: Reading,
    counted: [Reading; 5],
    readings: [u64; 5],
}

thread_local! {
    // The engine's hook is a plain function, so what it needs is kept here. Per thread: the counters are this thread's.
    static METER: RefCell<Option<Meter>> = const { RefCell::new(None) };
}

/// A boundary: reads the counters and books what was counted since the reading before to the part that ends here.
fn boundary(part: u8) {
    METER.with_borrow_mut(|meter| {
        let Some(meter) = meter else { return };
        let now = (meter.read)();
        (0..MAX).for_each(|k| meter.counted[part as usize][k] += now[k] - meter.last[k]);
        meter.readings[part as usize] += 1;
        meter.last = now;
    });
}

/// Sums per kind of step: [kind][part][event], the readings per [kind][part], and steps and ticks per kind.
#[derive(Default)]
struct Sums {
    counted: [[Reading; 5]; 5],
    readings: [[u64; 5]; 5],
    steps: [u64; 5],
    crossed: [u64; 5],
}

impl Sums {
    /// Moves what the meter holds for the steps just made to their kind.
    fn book(&mut self, kind: usize, steps: u64, crossed: u64) {
        METER.with_borrow_mut(|meter| {
            let meter = meter.as_mut().unwrap();
            for part in 0..5 {
                (0..MAX).for_each(|k| self.counted[kind][part][k] += meter.counted[part][k]);
                self.readings[kind][part] += meter.readings[part];
            }
            (meter.counted, meter.readings) = Default::default();
        });
        self.steps[kind] += steps;
        self.crossed[kind] += crossed;
    }
}

/// Has the processor count these events on this thread; what comes back reads them (the first N numbers).
fn open<const N: usize>(sampler: &'static Sampler, events: [Event; N]) -> Result<Box<dyn Fn() -> Reading>, String> {
    let mut thread = sampler.thread(events).map_err(|e| format!("{e:?}"))?;
    thread.start().map_err(|e| format!("{e:?}"))?;
    Ok(Box::new(move || {
        let mut all = [0; MAX];
        all[..N].copy_from_slice(&thread.sample().expect("reading the counters"));
        all
    }))
}

/// `open` for a list: the crate wants the number of events at compile time.
fn open_list(sampler: &'static Sampler, events: &[Event]) -> Result<Box<dyn Fn() -> Reading>, String> {
    macro_rules! sized {
        ($($n:literal)*) => {
            match events.len() {
                $($n => open::<$n>(sampler, events.try_into().unwrap()),)*
                n => Err(format!("no case for {n} events")),
            }
        };
    }
    sized!(2 3 4 5 6 7 8 9)
}

/// The sets of events to ask for, in order: the first that the processor accepts is used. The two fixed counters
/// (cycles, then instructions) are first in every set, and the report relies on that.
///
/// What the crate's table for the M5 says (darwin-kperf-events, m5.rs, `counters_mask`): of the eight counters
/// that can be set (numbers 2 to 9), every event that counts retired instructions (all the branch events here,
/// and the cache and TLB misses ending in `Nonspec`) can only use numbers 2 to 7, six counters; an event without
/// a mask (`L2TlbMissData`) can use any. So at most six of the restricted events fit: the usual set has four and
/// one free; the full branch set has seven and is expected to be refused, and without the indirect calls it has
/// six and should just fit. It is asked for all the same, since this is read from a table and not observed.
fn sets(which: &str) -> Result<Vec<Vec<(Event, &'static str)>>, String> {
    let fixed = [(Event::FixedCycles, "cycles"), (Event::FixedInstructions, "instructions"), (Event::BranchMispredNonspec, "branches mispredicted")];
    let with = |more: &[(Event, &'static str)]| fixed.iter().chain(more).copied().collect::<Vec<_>>();
    match which {
        "caches" => {
            let misses = [(Event::L1DCacheMissLdNonspec, "loads that missed the first cache"), (Event::L1DCacheMissStNonspec, "stores that missed the first cache"), (Event::L1DTlbMissNonspec, "first-level TLB misses (data)"), (Event::L2TlbMissData, "second-level TLB misses (data): page walks")];
            Ok(vec![with(&misses), with(&misses[..3])])
        }
        "branches" => {
            // Least important last in each half, so that the smaller sets are this one with pieces left out.
            let mispredicted = [(Event::BranchCondMispredNonspec, "conditional branches mispredicted"), (Event::BranchIndirMispredNonspec, "indirect branches mispredicted (with calls and returns)"), (Event::BranchRetIndirMispredNonspec, "returns mispredicted"), (Event::BranchCallIndirMispredNonspec, "indirect calls mispredicted")];
            let executed = [(Event::InstBranchCond, "conditional branches executed"), (Event::InstBranchIndir, "indirect branches executed (with indirect calls)")];
            Ok(vec![with(&[&mispredicted[..], &executed].concat()), with(&[&mispredicted[..3], &executed].concat()), with(&[&mispredicted[..2], &executed].concat()), with(&mispredicted[..2])])
        }
        _ => Err(format!("events={which}: not understood (caches, branches)")),
    }
}

fn measure(events: &[(Event, &str)], engine: &mut vcb::sim2::Engine, ticks: u64, every: u64, parts: bool) {
    let n = events.len();
    let names: Vec<&str> = events.iter().map(|e| e.1).collect();
    let restart = || METER.with_borrow_mut(|meter| meter.as_mut().map(|meter| meter.last = (meter.read)()));
    // What one reading costs, with the bookkeeping that goes with it: the mean of many empty stretches.
    let mut empty = [0f64; MAX];
    const EMPTY: u64 = 20_000;
    let mut stretches = Sums::default();
    restart();
    for _ in 0..EMPTY {
        std::hint::black_box((engine.split_counts(), engine.replay_stats(), engine.tick()));
        boundary(END as u8);
        stretches.book(4, 1, 0);
    }
    (0..n).for_each(|k| empty[k] = stretches.counted[4][END][k] as f64 / EMPTY as f64);
    let mut sums = Sums::default();
    let (mut counts, mut replayed, mut tick) = (engine.split_counts(), engine.replay_stats().2, engine.tick());
    restart();
    while engine.tick() < ticks && !engine.breakpoint_hit() {
        let mut done = 0;
        while done < every && engine.tick() < ticks && !engine.breakpoint_hit() {
            engine.advance(ticks);
            done += 1;
        }
        boundary(END as u8);
        let (counts_now, replayed_now, tick_now) = (engine.split_counts(), engine.replay_stats().2, engine.tick());
        let kind = if every > 1 {
            4
        } else if replayed_now != replayed {
            0
        } else if counts_now.split_steps != counts.split_steps {
            1
        } else if counts_now.whole_steps != counts.whole_steps {
            2
        } else {
            3
        };
        sums.book(kind, done, tick_now - tick);
        (counts, replayed, tick) = (counts_now, replayed_now, tick_now);
    }
    println!("to tick {}; one reading of the counters costs {} (taken off below, once per reading)", engine.tick(), (0..n).map(|k| format!("{:.0} {}", empty[k], names[k])).collect::<Vec<_>>().join(", "));
    println!("{:?}", engine.split_counts());
    if parts {
        println!("the parts of a step (a reading at the end of each; the cache and TLB misses are disturbed more by these than by one reading per step, instructions and mispredicted branches are not):");
        PARTS.iter().for_each(|(name, what)| println!("    {name:<12} {what}"));
    }
    // One event of one part of one kind, per step of that kind, with the readings' own cost taken off.
    let per_part = |kind: usize, part: usize, k: usize| ((sums.counted[kind][part][k] as f64 - empty[k] * sums.readings[kind][part] as f64) / sums.steps[kind].max(1) as f64).max(0.0);
    let per = |kind: usize, k: usize| (0..5).map(|part| per_part(kind, part, k)).sum::<f64>();
    let all: f64 = (0..5).map(|kind| per(kind, 0) * sums.steps[kind] as f64).sum();
    let place = |event: Event| events.iter().position(|e| e.0 == event);
    for kind in 0..5 {
        let steps = sums.steps[kind];
        if steps == 0 {
            continue;
        }
        println!();
        println!("{}: {} steps of {:.0} ticks; {:.1}% of the cycles", KINDS[kind], steps, sums.crossed[kind] as f64 / steps as f64, 100.0 * per(kind, 0) * steps as f64 / all.max(1.0));
        for k in 0..n {
            println!("    {:<56} {:>12.2} per step {:>10.3} per 1000 instructions", names[k], per(kind, k), 1000.0 * per(kind, k) / per(kind, 1).max(1.0));
        }
        println!("    instructions per cycle: {:.2}", per(kind, 1) / per(kind, 0).max(1.0));
        // The rates per branch, where both numbers were counted.
        if let (Some(missed), Some(executed)) = (place(Event::BranchCondMispredNonspec), place(Event::InstBranchCond)) {
            println!("    conditional branches: {:.2} per step, {:.3}% of them mispredicted", per(kind, executed), 100.0 * per(kind, missed) / per(kind, executed).max(1e-9));
        }
        if let (Some(missed), Some(executed)) = (place(Event::BranchIndirMispredNonspec), place(Event::InstBranchIndir)) {
            let returns = place(Event::BranchRetIndirMispredNonspec).map_or(0.0, |k| per(kind, k));
            println!("    indirect branches: {:.2} per step, {:.3}% of them mispredicted ({})", per(kind, executed), 100.0 * (per(kind, missed) - returns).max(0.0) / per(kind, executed).max(1e-9), if place(Event::BranchRetIndirMispredNonspec).is_some() { "the mispredicted returns taken off" } else { "the mispredicted ones may include returns, the executed ones may not" });
        }
        if parts && kind != 0 {
            println!("    per step, by part:");
            let row = |name: &str, value: &dyn Fn(usize) -> String| println!("    {:<56} {}", name, (0..5).map(|part| format!("{:>12}", value(part))).collect::<String>());
            row("", &|part| PARTS[part].0.to_string());
            row("readings", &|part| format!("{:.3}", sums.readings[kind][part] as f64 / steps as f64));
            for k in 0..n {
                row(names[k], &|part| format!("{:.2}", per_part(kind, part, k)));
            }
        }
    }
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        return Err("usage: sudo counters <board.vcb> <program.vcbasm or -> [ticks] [every] [nosplit] [table=<MiB>] [events=caches|branches] [parts] [dry]".into());
    }
    let mut board = vcb::Board::read(Path::new(&args[1]))?;
    let source = if args[2] == "-" { board.assembly.clone() } else { std::fs::read_to_string(&args[2]).map_err(|e| e.to_string())?.replace("\r\n", "\n") };
    vcb::asm::assemble(&source, &args[2], &mut board, true)?;
    let ticks: u64 = args.get(3).map_or(300_000_000, |t| t.parse().unwrap());
    let every: u64 = args.get(4).map_or(1, |t| t.parse().unwrap());
    let options = vcb::sim::Options { timer_ticks: Some(1000) };
    let components = Components::build(&board)?;
    // Further words: `nosplit` (no records: every step that replay misses is simulated whole) and `table=<MiB>` (a
    // limit on the replay table, so that it stays small enough for the caches), to see what each has to do with the
    // misses; `events=<set>` and `parts`, see the top of this file.
    let (mut which, mut parts, mut dry) = ("caches", false, false);
    let mut engine = vcb::sim2::Engine::new(&board, components, &options)?;
    for word in args.iter().skip(5) {
        match word.split_once('=') {
            None if word == "nosplit" => engine.set_split(false),
            None if word == "parts" => parts = true,
            None if word == "dry" => dry = true,
            Some(("table", size)) => engine.set_memory_limit(size.parse::<usize>().map_err(|e| e.to_string())? << 20),
            Some(("events", set)) => which = set,
            _ => return Err(format!("{word}: not understood (nosplit, table=<MiB>, events=caches|branches, parts, dry)")),
        }
    }
    let sets = sets(which)?;
    if parts {
        #[cfg(not(parts))]
        return Err("parts: this binary's engine has no calls at the parts of a step; build it with RUSTFLAGS=\"--cfg parts -C target-cpu=native\" cargo build --release --example counters --target-dir target/parts".into());
        // Safety: nothing has run yet, and the worker thread never calls it (and would find no meter if it did).
        #[cfg(parts)]
        unsafe {
            vcb::sim2::PART_HOOK = boundary
        };
    }
    let run = |set: &[(Event, &'static str)], read: Box<dyn Fn() -> Reading>, engine: &mut vcb::sim2::Engine| {
        METER.set(Some(Meter { read, last: [0; MAX], counted: Default::default(), readings: Default::default() }));
        measure(set, engine, ticks, every, parts);
        // Dropped here, which stops the counting.
        METER.take();
    };
    if dry {
        println!("DRY: nothing is counted, every figure below is nanoseconds of the clock");
        let start = std::time::Instant::now();
        run(&sets[0], Box::new(move || [start.elapsed().as_nanos() as u64; MAX]), &mut engine);
        return Ok(());
    }
    // Leaked, so that what reads the counters can be kept where the engine's hook finds it.
    let sampler: &'static Sampler = Box::leak(Box::new(Sampler::new().map_err(|e| format!("the counters could not be opened ({e:?}): this needs root, run it with sudo"))?));
    println!("processor: {:?}", sampler.cpu());
    for (tried, set) in sets.iter().enumerate() {
        let events: Vec<Event> = set.iter().map(|e| e.0).collect();
        match open_list(sampler, &events) {
            Ok(read) => {
                println!("counting {} events (set {} of {} tried in order): {}", set.len(), tried + 1, sets.len(), set.iter().map(|e| e.1).collect::<Vec<_>>().join("; "));
                run(set, read, &mut engine);
                return Ok(());
            }
            Err(error) => println!("{} events were refused ({error}){}", set.len(), if tried + 1 < sets.len() { "; trying a smaller set" } else { "" }),
        }
    }
    Err("the processor accepted none of the sets of events".into())
}
