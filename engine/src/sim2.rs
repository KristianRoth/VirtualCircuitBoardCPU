//! The v2 engine: the fast one. It must show the same events as v1 (specs/vcb-engine-v2.md); every change
//! to it is an entry in specs/vcb-engine-v2-log.md. It started as a copy of v1's core (attempt 0).

use crate::board::Board;
use crate::format::Result;
use crate::ink::Ink;
use crate::netlist::{Components, Kind};
use crate::replay::{Arrival, Edge, Meta, Table, NONE};
use crate::sim::{Options, Rng, Simulation};

/// All of v2's tables and caches together stay under this many bytes (the spec's limit). `set_memory_limit` lowers it.
const MEMORY_LIMIT: usize = 2 << 30;

mod split;

/// Indexing without the bounds check, for the tick's inner loops (attempt 14). Every index there is an entity
/// number or a position taken from the netlist's own lists, all made in `Engine::new` for arrays of that size;
/// a debug build still checks them.
#[inline(always)]
fn g<T: Copy>(v: &[T], i: usize) -> T {
    debug_assert!(i < v.len());
    // Safety: see above.
    unsafe { *v.get_unchecked(i) }
}

#[inline(always)]
fn gm<T>(v: &mut [T], i: usize) -> &mut T {
    debug_assert!(i < v.len());
    // Safety: see above.
    unsafe { v.get_unchecked_mut(i) }
}

/// An entity as the tick's loops see it.
#[derive(Clone, Copy)]
struct Cell {
    /// Per component: how many of the traces it reads are on. Per trace: how many of its writers are on.
    count: u16,
    /// Per gate whose output depends only on the count passing one value: that value (1 for OR and NOR, the
    /// number of inputs for AND and NAND). Such a gate is woken only when its count passes it.
    threshold: u16,
    op: Op,
    /// A trace is on when any writer is on; a component's value is its output.
    on: bool,
    /// Waiting to be evaluated in the next tick.
    queued: bool,
    /// Per latch or pin: whether the inputs that rose since it was last evaluated would toggle it.
    toggle: bool,
}

/// Many short lists in one array (attempt 3): the list of entity e is `items[start[e]..start[e + 1]]`.
#[derive(Clone)]
struct Lists {
    start: Vec<u32>,
    items: Vec<u32>,
}

impl Lists {
    fn new(lists: &[Vec<u32>]) -> Lists {
        let mut start = vec![0u32];
        let mut items = Vec::new();
        for list in lists {
            items.extend(list);
            start.push(items.len() as u32);
        }
        Lists { start, items }
    }

    #[inline(always)]
    fn of(&self, e: usize) -> &[u32] {
        debug_assert!(e + 1 < self.start.len() && self.start[e + 1] as usize <= self.items.len());
        // Safety: `start` has one entry more than there are entities and rises up to `items.len()`.
        unsafe { self.items.get_unchecked(*self.start.get_unchecked(e) as usize..*self.start.get_unchecked(e + 1) as usize) }
    }
}

/// The part of a board that runs by itself (attempt 11): everything a clock drives that no VMem pin, Random or
/// Timer can ever influence. Its behaviour is fixed from tick 0 and soon repeats, so it is run once, before
/// the simulation, and then only the flips it hands to the rest of the circuit are played from this schedule.
#[derive(Default, Clone)]
struct Schedule {
    /// `at[k]`: the components that flip in tick k, for k up to `lead + period`. Later ticks repeat the last `period`.
    at: Vec<Vec<u32>>,
    /// From k: how many ticks until the next tick with a flip (0 if never).
    wait: Vec<u64>,
    lead: u64,
    period: u64,
}

impl Schedule {
    /// The position of a tick in the schedule: itself while the start-up lasts, then within the period.
    #[inline(always)]
    fn key(&self, tick: u64) -> usize {
        if tick <= self.lead + self.period { tick as usize } else { (self.lead + 1 + (tick - self.lead - 1) % self.period) as usize }
    }

    /// The next tick after `tick` in which the schedule flips something.
    fn next(&self, tick: u64) -> u64 {
        if self.at.is_empty() || self.wait[self.key(tick)] == 0 { u64::MAX } else { tick + self.wait[self.key(tick)] }
    }

    /// Finds the self-running part and runs it. `keep` gets true for its entities that the rest of the circuit
    /// reads (they stay, driven by the schedule); `drop` for the others (not simulated at all).
    fn find(board: &Board, c: &Components, options: &Options, live: &[bool], keep: &mut [bool], drop: &mut [bool]) -> Result<Schedule> {
        /// How long the self-running part may take to repeat. Beyond this it is simulated like everything else.
        const LIMIT: u64 = 1 << 16;
        let n = c.kinds.len();
        let reach = |from: &dyn Fn(&Kind) -> bool| {
            let mut seen = vec![false; n];
            let mut todo: Vec<usize> = (0..n).filter(|e| from(&c.kinds[*e])).collect();
            while let Some(e) = todo.pop() {
                if !std::mem::replace(&mut seen[e], true) {
                    // A clock ignores its inputs, so nothing reaches further through one.
                    let next = if c.kinds[e] == Kind::Trace { &c.readers[e] } else { &c.writes[e] };
                    todo.extend(next.iter().map(|x| *x as usize).filter(|x| c.kinds[*x] != Kind::Component(Ink::Clock)));
                }
            }
            seen
        };
        let influenced = reach(&|k| matches!(k, Kind::Pin | Kind::Component(Ink::Random | Ink::Timer)));
        let driven = reach(&|k| *k == Kind::Component(Ink::Clock));
        let own: Vec<bool> = (0..n).map(|e| live[e] && driven[e] && !influenced[e]).collect();
        // What the rest reads stays: a trace of the part with a reader outside it, and every component that
        // writes such a trace or a trace outside the part.
        let read_outside = |t: usize| c.readers[t].iter().any(|r| live[*r as usize] && !own[*r as usize]);
        for e in (0..n).filter(|e| own[*e]) {
            keep[e] = if c.kinds[e] == Kind::Trace { read_outside(e) } else { c.writes[e].iter().any(|t| live[*t as usize] && (!own[*t as usize] || read_outside(*t as usize))) };
            drop[e] = !keep[e];
        }
        let kept: Vec<u32> = (0..n as u32).filter(|e| keep[*e as usize] && c.kinds[*e as usize] != Kind::Trace).collect();
        let members: Vec<usize> = (0..n).filter(|e| own[*e]).collect();
        if members.is_empty() {
            return Ok(Schedule::default());
        }
        // Run the reference engine until the part is in a situation it was in before. Its situation is its
        // signals now and one tick ago (which fixes what is pending) and the position in the clock interval.
        let mut v1 = crate::sim::Engine::new(board, c.clone(), options)?;
        let interval = board.settings.clock_interval as u64;
        let mut at: Vec<Vec<u32>> = vec![Vec::new()];
        let mut seen: std::collections::HashMap<(Vec<bool>, Vec<bool>, u64), u64> = Default::default();
        let mut before: Vec<bool> = members.iter().map(|e| v1.state()[*e]).collect();
        let mut kept_before: Vec<bool> = kept.iter().map(|e| v1.state()[*e as usize]).collect();
        for tick in 1..=LIMIT {
            v1.step();
            let now: Vec<bool> = members.iter().map(|e| v1.state()[*e]).collect();
            let kept_now: Vec<bool> = kept.iter().map(|e| v1.state()[*e as usize]).collect();
            at.push(kept.iter().zip(kept_now.iter().zip(&kept_before)).filter(|(_, (a, b))| a != b).map(|(e, _)| *e).collect());
            kept_before = kept_now;
            if let Some(first) = seen.insert((now.clone(), std::mem::replace(&mut before, now), tick % interval), tick) {
                let (lead, period) = (first, tick - first);
                // wait[k]: ticks from k to the next flip. After the last position comes position lead + 1 again.
                let last = at.len() - 1;
                let mut wait = vec![0u64; at.len()];
                wait[last] = (lead as usize + 1..=last).find(|k| !at[*k].is_empty()).map_or(0, |k| k as u64 - lead);
                for k in (0..last).rev() {
                    wait[k] = if !at[k + 1].is_empty() { 1 } else if wait[k + 1] > 0 { wait[k + 1] + 1 } else { 0 };
                }
                return Ok(Schedule { at, wait, lead, period });
            }
        }
        // No repeat found: leave the part to the ordinary simulation.
        for e in members {
            (keep[e], drop[e]) = (false, false);
        }
        Ok(Schedule::default())
    }
}

/// What an entity does when it is evaluated: one byte per entity instead of the netlist's kind and ink.
#[derive(Clone, Copy, PartialEq)]
#[repr(u8)]
enum Op {
    /// On when any input is on: Buffer, OR, LED, Wireless.
    Or,
    /// The same, and a Breakpoint.
    Breakpoint,
    /// Not and NOR.
    Nor,
    And,
    Nand,
    Xor,
    Xnor,
    /// Latch On, Latch Off, and a VMem pin when VMem is not locked.
    Latch,
    Pin,
    /// Clock and Timer: flipped by the schedule, never by their inputs.
    Scheduled,
    Random,
    Trace,
    /// A component that cannot reach a VMem pin, a Breakpoint or a Random: never evaluated (attempt 6).
    Dead,
}

/// What the engine did, counted only in a profiling build (`RUSTFLAGS="--cfg profiling"`): for the profile of
/// v2.1 (specs/vcb-engine-v2.1.md). In an ordinary build the counters stay 0 and cost nothing.
#[derive(Default, Clone, Copy, Debug)]
pub struct Profile {
    /// Ticks simulated one by one, and the flips in them.
    pub ticks_simulated: u64,
    pub flips: u64,
    /// Components evaluated.
    pub evaluations: u64,
    /// Ticks jumped over because nothing was queued.
    pub ticks_idle: u64,
    /// Steps taken from the replay table, states looked up or stored, states restored into the circuit.
    pub replay_steps: u64,
    /// How often a flip was applied to something that reads it, split by what the reader is and whether that
    /// could change its output: [AND-like or OR-like gate woken, such a gate not woken, XOR, latch or pin
    /// woken, latch or pin not woken, anything else].
    pub feeds: [u64; 6],
    /// Traces that changed, and writer counts adjusted for traces with several writers.
    pub trace_flips: u64,
    pub trace_counts: u64,
    pub interns: u64,
    pub restores: u64,
}

#[derive(Clone)]
pub struct Engine {
    pub components: Components,
    pub profile: Profile,
    /// What the tick's loops need of an entity, in one place: eight bytes, one cache line for eight entities
    /// (attempt 20). It was six arrays before.
    cell: Vec<Cell>,
    /// The signals as `state()` shows them: a copy of the cells' `on`, brought up to date by `sync` and `step`.
    view: Vec<bool>,
    reads: Lists,
    writes: Lists,
    readers: Lists,
    writers: Lists,
    /// The self-running part's flips (attempt 11). Empty when the board has none.
    schedule: Schedule,
    /// Per component or pin: the readers of the traces that only it writes. Such a trace always equals its
    /// writer, so it is not simulated: its readers read the writer itself (attempt 10).
    direct: Lists,
    /// Per entity: whether it is simulated: it can reach a VMem pin, a Breakpoint or a Random, and it is not a
    /// trace with one writer (which always equals that writer). For the others `state()` is not meaningful.
    live: Vec<bool>,
    /// The simulated signals as one bit each, in `live_list` order, kept up to date at every change, so that a
    /// state is ready to be looked up without being packed (attempt 19). `slot` is an entity's bit; an entity
    /// that is not simulated points at a spare word after the real ones.
    packed: Vec<u64>,
    slot: Vec<u32>,
    /// Components to evaluate on the next tick.
    dirty: Vec<u32>,
    /// The two lists a tick works on, kept between ticks so that a tick allocates nothing (attempt 1).
    work: Vec<u32>,
    flips: Vec<u32>,
    clocks: Vec<u32>,
    /// The next tick at which the clocks, the timers and the schedule flip something (u64::MAX: never). Kept
    /// so that a tick does not divide to find out (attempt 13). `set_due` renews them when the tick counter
    /// moves by more than a step.
    clock_due: u64,
    timer_due: u64,
    schedule_due: u64,
    clock_interval: u64,
    timers: Vec<u32>,
    timer_ticks: u64,
    rng: Rng,
    tick: u64,
    breakpoint_hit: bool,
    vmem: Vec<u32>,
    /// The address the address pins held at the end of the last tick.
    address: usize,
    /// VMem rejects input to its pins in the tick after an address change.
    locked: bool,
    /// The word at the new address is loaded into the content pins in that same locked tick.
    load_pending: bool,
    /// The word the last load put on the content pins.
    loaded_word: u32,
    /// Replay (attempt 8). `None` on a board with Random ink: every evaluation of a Random uses up a draw.
    replay: Option<Table>,
    /// The simulated entities, in entity order: a node stores one bit for each.
    live_list: Vec<u32>,
    /// The node the engine was at when the tick counter was `node_tick` (NONE: not known).
    node: u32,
    node_tick: u64,
    /// False while replay has moved on without the circuit: the signals are then those of an earlier node.
    real: bool,
    /// The node whose state the circuit's arrays hold exactly, if any: restoring another node then only
    /// touches what differs between the two.
    arrays_node: u32,
    /// After a node was restored: the components that flip in the next tick. A node stores these and not the
    /// queue and the just-risen marks they follow from, so that states which behave alike are one node.
    forced: Vec<u32>,
    pending: Vec<u32>,
    flush_when_full: bool,
    /// What VMem did in the last tick: the address loaded from, the address written to, and whether a word changed.
    fetched: Option<usize>,
    written: Option<usize>,
    vmem_changed: bool,
    /// Per entity: 1 for an address pin, 2 for a content pin, 0 otherwise.
    pin_role: Vec<u8>,
    /// The fixed part of a step applied from a record (attempt 27, `sim2/split.rs`).
    split: split::Split,
}

impl Engine {
    /// The reset state (tick 0).
    pub fn new(board: &Board, components: Components, options: &Options) -> Result<Engine> {
        let clock_interval = u64::try_from(board.settings.clock_interval).ok().filter(|n| *n > 0).ok_or("clock_interval: must be at least 1")?;
        let n = components.kinds.len();
        let has_timer = components.kinds.contains(&Kind::Component(Ink::Timer));
        let timer_ticks = match options.timer_ticks {
            Some(0) => return Err("--timer-ticks: must be at least 1".into()),
            Some(n) => n,
            None if has_timer => return Err("the board has Timer ink: --timer-ticks <n> is required, because the game's timers follow the wall clock".into()),
            None => 1,
        };
        let rng = Rng::new(board.settings.random_seed as u32);
        let ops = components.kinds.iter().map(|kind| match kind {
            Kind::Trace => Op::Trace,
            Kind::Pin => Op::Pin,
            Kind::Component(ink) => match ink {
                Ink::Buffer | Ink::Or | Ink::Led | Ink::Wireless(_) => Op::Or,
                Ink::Breakpoint => Op::Breakpoint,
                Ink::Not | Ink::Nor => Op::Nor,
                Ink::And => Op::And,
                Ink::Nand => Op::Nand,
                Ink::Xor => Op::Xor,
                Ink::Xnor => Op::Xnor,
                Ink::LatchOn | Ink::LatchOff => Op::Latch,
                Ink::Clock | Ink::Timer => Op::Scheduled,
                Ink::Random => Op::Random,
                other => unreachable!("{other} is not a component"),
            },
        }).collect();
        let (reads, writes, readers, writers) = (Lists::new(&components.reads), Lists::new(&components.writes), Lists::new(&components.readers), Lists::new(&components.writers));
        // Only what can reach a VMem pin, a Breakpoint or a Random can cause an event (a Random matters because
        // each of its evaluations uses up a draw). Walk backwards from those: a component needs the traces it
        // reads, a trace needs its writers.
        let mut ops: Vec<Op> = ops;
        let mut live = vec![false; n];
        let mut todo: Vec<usize> = (0..n).filter(|e| matches!(ops[*e], Op::Pin | Op::Breakpoint | Op::Random)).collect();
        while let Some(e) = todo.pop() {
            if !std::mem::replace(&mut live[e], true) {
                todo.extend((if ops[e] == Op::Trace { writers.of(e) } else { reads.of(e) }).iter().map(|i| *i as usize));
            }
        }
        for e in 0..n {
            if !live[e] && ops[e] != Op::Trace {
                ops[e] = Op::Dead;
            }
        }
        // The self-running part (attempt 11): not on a board with Random ink, where every flip must come in v1's order.
        let (mut keep, mut dropped) = (vec![false; n], vec![false; n]);
        let schedule = if ops.contains(&Op::Random) { Schedule::default() } else { Schedule::find(board, &components, options, &live, &mut keep, &mut dropped)? };
        for e in 0..n {
            if dropped[e] {
                live[e] = false;
                if ops[e] != Op::Trace {
                    ops[e] = Op::Dead;
                }
            } else if keep[e] && ops[e] != Op::Trace {
                ops[e] = Op::Scheduled; // flipped by the schedule, never evaluated
            }
        }
        // Netlist optimisation (attempt 10). A trace with one writer always equals that writer, in the same
        // tick, so it is taken out: whoever read the trace reads the writer. Lists also lose what is not
        // simulated. Not on a board with Random ink, where the order in which readers are woken must be v1's.
        let merge = !ops.contains(&Op::Random);
        let merged: Vec<bool> = (0..n).map(|e| merge && live[e] && ops[e] == Op::Trace && writers.of(e).len() == 1).collect();
        let simulated = |e: &u32| ops[*e as usize] != Op::Dead;
        let per_entity = |list: &dyn Fn(usize) -> Vec<u32>| Lists::new(&(0..n).map(list).collect::<Vec<_>>());
        let direct = per_entity(&|e| writes.of(e).iter().filter(|t| merged[**t as usize]).flat_map(|t| readers.of(*t as usize).iter().filter(|r| simulated(r)).copied()).collect());
        let (reads, writes, readers) = if merge {
            (
                per_entity(&|e| reads.of(e).iter().map(|t| if merged[*t as usize] { writers.of(*t as usize)[0] } else { *t }).collect()),
                per_entity(&|e| writes.of(e).iter().filter(|t| live[**t as usize] && !merged[**t as usize]).copied().collect()),
                per_entity(&|e| readers.of(e).iter().filter(|r| simulated(r)).copied().collect()),
            )
        } else {
            (reads, writes, readers)
        };
        for e in 0..n {
            live[e] &= !merged[e];
        }
        let live_list: Vec<u32> = (0..n as u32).filter(|e| live[*e as usize]).collect();
        // Replay is off with Random ink (each evaluation uses up a draw). A simulated Timer does not switch it
        // off (attempt 23): a step in which no timer flips does not depend on where the timers are in their
        // interval, so that position stays out of the states, and only such steps are recorded and replayed.
        let replay = (!ops.contains(&Op::Random)).then(|| Table::new(live_list.len().div_ceil(64), MEMORY_LIMIT));
        let mut engine = Engine { profile: Profile::default(), loaded_word: 0, replay, live_list, node: NONE, node_tick: 0, real: true, arrays_node: NONE, forced: Vec::new(), pending: Vec::new(), flush_when_full: true, schedule, cell: ops.iter().map(|op| Cell { count: 0, threshold: 0, op: *op, on: false, queued: false, toggle: false }).collect(), view: vec![false; n], reads, writes, readers, writers, direct, live, packed: Vec::new(), slot: Vec::new(), dirty: Vec::new(), work: Vec::new(), flips: Vec::new(),  clocks: Vec::new(), clock_due: u64::MAX, timer_due: u64::MAX, schedule_due: u64::MAX, clock_interval, timers: Vec::new(), timer_ticks, rng, tick: 0, breakpoint_hit: false, vmem: board.vmem.clone(), address: 0, locked: false, load_pending: false, fetched: None, written: None, vmem_changed: false, pin_role: vec![0; n], split: Default::default(), components };
        for (role, pins) in [(1, &engine.components.address_pins), (2, &engine.components.content_pins)] {
            for pin in pins {
                engine.pin_role[*pin as usize] = role;
            }
        }
        for e in 0..n {
            if engine.cell[e].op == Op::Breakpoint {
                engine.pin_role[e] |= 4;
            }
        }
        let c = &engine.components;
        // Observed (gates board): at tick 0 every component already has the output it gets with all
        // inputs off, so Latch On and the inverting gates start on.
        for (e, kind) in c.kinds.iter().enumerate() {
            if engine.cell[e].op == Op::Dead {
                continue;
            }
            match kind {
                // Observed (general boards): a clock is on at tick 0 and flips at every multiple of the interval.
                Kind::Component(Ink::Clock) => {
                    if !keep[e] {
                        engine.clocks.push(e as u32);
                    }
                    engine.cell[e].on = true;
                }
                // Observed (random-pulse board): unlike a clock, a timer starts off.
                Kind::Component(Ink::Timer) => engine.timers.push(e as u32),
                Kind::Component(Ink::LatchOn) => engine.cell[e].on = true,
                // (Written out, not evaluated: a component on the schedule has no operation to evaluate.)
                Kind::Component(Ink::Not | Ink::Nor | Ink::Xnor) => engine.cell[e].on = true,
                Kind::Component(Ink::Nand) => engine.cell[e].on = !c.reads[e].is_empty(),
                _ => {}
            }
        }
        // Their traces are on at tick 0 too, and count as having just risen: a latch reading one toggles at tick 1.
        for e in 0..n {
            if c.kinds[e] == Kind::Trace && c.writers[e].iter().any(|w| engine.cell[*w as usize].on) {
                engine.cell[e].on = true;
                for &reader in &c.readers[e] {
                    if matches!(engine.cell[reader as usize].op, Op::Latch | Op::Pin) {
                        engine.cell[reader as usize].toggle ^= true;
                    }
                    if !engine.cell[reader as usize].queued && engine.cell[reader as usize].op != Op::Dead {
                        engine.cell[reader as usize].queued = true;
                        engine.dirty.push(reader);
                    }
                }
            }
        }
        engine.set_due();
        let words = engine.live_list.len().div_ceil(64);
        engine.slot = vec![(words * 64) as u32; n];
        engine.packed = vec![0; words + 1];
        for (i, &e) in engine.live_list.iter().enumerate() {
            engine.slot[e as usize] = i as u32;
            engine.packed[i >> 6] |= (engine.cell[e as usize].on as u64) << (i & 63);
        }
        for e in 0..n {
            let inputs = if engine.cell[e].op == Op::Trace { engine.writers.of(e) } else { engine.reads.of(e) };
            engine.cell[e].count = inputs.iter().filter(|i| engine.cell[**i as usize].on).count() as u16;
            engine.cell[e].threshold = if matches!(engine.cell[e].op, Op::And | Op::Nand) { inputs.len() as u16 } else { 1 };
            if inputs.len() > u16::MAX as usize {
                return Err(format!("an entity with {} inputs: the v2 engine counts up to 65,535 (use --engine v1)", inputs.len()));
            }
        }
        (engine.split.due, engine.split.threads) = (u64::MAX, 1);
        engine.set_split(true);
        engine.show();
        Ok(engine)
    }

    /// The output a component would have after this tick, from the trace states of the previous tick.
    #[inline(always)]
    fn evaluate(&self, e: usize, op: Op) -> bool {
        let cell = g(&self.cell, e);
        // For AND and NAND the threshold is the number of inputs.
        let (active, inputs) = (cell.count, cell.threshold);
        match op {
            Op::Or | Op::Breakpoint => active > 0,
            Op::Nor => active == 0,
            Op::And => active > 0 && active == inputs,
            // Observed: a NAND with no inputs is off, so this is not the exact inverse of AND.
            Op::Nand => active < inputs,
            Op::Xor => active % 2 == 1,
            Op::Xnor => active % 2 == 0,
            // Observed: toggles once per input that rose, so two inputs rising together cancel out.
            Op::Latch | Op::Pin => g(&self.cell, e).on != g(&self.cell, e).toggle,
            // Clocks and timers flip from the schedule in `step`, never from their inputs (observed).
            Op::Scheduled => g(&self.cell, e).on,
            Op::Random | Op::Trace | Op::Dead => unreachable!("not evaluated here"),
        }
    }
}

impl Engine {
    /// Per entity: whether v2 simulates it. `state()` is only meaningful for these.
    pub fn live(&self) -> &[bool] {
        &self.live
    }

    /// Per entity: whether it is flipped by the clock schedule and never evaluated (attempt 11).
    pub fn scheduled(&self) -> Vec<bool> {
        self.cell.iter().map(|c| c.op == Op::Scheduled).collect()
    }
}

impl Engine {
    /// The number held by a group of pins, bit 0 first.
    fn pins_value(&self, pins: &[u32]) -> u32 {
        pins.iter().enumerate().map(|(k, pin)| (self.cell[*pin as usize].on as u32) << k).sum()
    }

    /// After components or pins flipped: recompute the traces they write, mark the ones that rose,
    /// and queue the readers of every trace that changed for the next tick.
    #[cfg_attr(profiling, inline(never))]
    fn propagate(&mut self, flipped: &[u32]) {
        // First every writer count, then the traces: a trace that loses one writer and gains another in the
        // same tick does not change, exactly as when all its writers are looked at afterwards.
        for &e in flipped {
            let step = if g(&self.cell, e as usize).on { 1 } else { u16::MAX }; // +1 or -1
            for &t in self.writes.of(e as usize) {
                let c = &mut gm(&mut self.cell, t as usize).count;
                *c = c.wrapping_add(step);
                if cfg!(profiling) {
                    self.profile.trace_counts += 1;
                }
            }
        }
        for &e in flipped {
            // The traces only this component writes: their readers see the component itself.
            let (from, to) = (g(&self.direct.start, e as usize) as usize, g(&self.direct.start, e as usize + 1) as usize);
            if from != to {
                let now = g(&self.cell, e as usize).on;
                for i in from..to {
                    self.feed(g(&self.direct.items, i), now);
                }
            }
            let (from, to) = (g(&self.writes.start, e as usize) as usize, g(&self.writes.start, e as usize + 1) as usize);
            for i in from..to {
                let t = g(&self.writes.items, i);
                let now = g(&self.cell, t as usize).count > 0;
                if now == g(&self.cell, t as usize).on {
                    continue;
                }
                gm(&mut self.cell, t as usize).on = now;
                self.flip_bit(t);
                if cfg!(profiling) {
                    self.profile.trace_flips += 1;
                }
                for k in g(&self.readers.start, t as usize) as usize..g(&self.readers.start, t as usize + 1) as usize {
                    self.feed(g(&self.readers.items, k), now);
                }
            }
        }
    }

    /// Keeps `packed` in step with a signal that just changed.
    #[inline(always)]
    fn flip_bit(&mut self, e: u32) {
        if self.replay.is_some() {
            let slot = g(&self.slot, e as usize) as usize;
            *gm(&mut self.packed, slot >> 6) ^= 1 << (slot & 63);
        }
    }

    /// One input of `reader` turned on (`now`) or off: adjusts its count and queues it if its output can change.
    #[inline(always)]
    fn feed(&mut self, reader: u32, now: bool) {
        let old = g(&self.cell, reader as usize).count;
        let new = old.wrapping_add(if now { 1 } else { u16::MAX });
        gm(&mut self.cell, reader as usize).count = new;
        // A gate that is not queued has the output its count gives. It can only change when the count
        // passes its threshold: the higher of the two counts is then the threshold.
        let wake = match g(&self.cell, reader as usize).op {
            Op::Or | Op::Breakpoint | Op::Nor | Op::And | Op::Nand => old.max(new) == g(&self.cell, reader as usize).threshold,
            Op::Xor | Op::Xnor | Op::Random => true, // a Random draws at every change of an input
            // Only a rising input toggles a latch; two rising together cancel out (observed).
            Op::Latch | Op::Pin => {
                if now {
                    let toggle = &mut gm(&mut self.cell, reader as usize).toggle;
                    *toggle = !*toggle;
                }
                now
            }
            Op::Scheduled | Op::Trace | Op::Dead => false,
        };
        if cfg!(profiling) {
            let kind = match g(&self.cell, reader as usize).op {
                Op::Or | Op::Breakpoint | Op::Nor | Op::And | Op::Nand => 1 - wake as usize,
                Op::Xor | Op::Xnor | Op::Random => 2,
                Op::Latch | Op::Pin => 4 - wake as usize,
                _ => 5,
            };
            self.profile.feeds[kind] += 1;
        }
        if wake && !g(&self.cell, reader as usize).queued {
            gm(&mut self.cell, reader as usize).queued = true;
            self.dirty.push(reader);
        }
    }
}

impl Engine {
    /// What VMem does in a tick in which a pin flipped, a load is pending or VMem is locked. Rare (about one
    /// tick in a hundred and fifty on a CPU), so it is kept out of the tick's own code (attempt 16).
    #[cold]
    #[inline(never)]
    fn vmem_tick(&mut self, pins_flipped: u8) {
        if self.load_pending {
            self.load_pending = false;
            self.fetched = Some(self.address);
            let word = self.vmem[self.address];
            self.loaded_word = word;
            let mut loaded = Vec::new();
            for (k, &pin) in self.components.content_pins.iter().enumerate() {
                if self.cell[pin as usize].on != (word >> k & 1 == 1) {
                    self.cell[pin as usize].on = !self.cell[pin as usize].on;
                    let slot = self.slot[pin as usize] as usize;
                    self.packed[slot >> 6] ^= 1 << (slot & 63);
                    loaded.push(pin);
                }
            }
            if self.split.mode == split::SPLIT {
                self.propagate_split(&loaded, false);
            } else {
                self.propagate(&loaded);
            }
            if self.split.mode != split::NORMAL {
                self.split.loaded = loaded;
            }
        }
        let (address_flipped, content_flipped) = (pins_flipped & 1 != 0, pins_flipped & 2 != 0);
        if content_flipped {
            // Bits above the content width are written as 0 (observed).
            let value = self.pins_value(&self.components.content_pins);
            self.written = Some(self.address);
            self.vmem_changed = self.vmem[self.address] != value;
            self.vmem[self.address] = value;
        }
        if address_flipped {
            self.address = self.pins_value(&self.components.address_pins) as usize;
            self.load_pending = true;
        }
        self.locked = address_flipped;
    }

    /// A Random's evaluation. Observed (random recorder boards): a Random draws every time it is evaluated, that
    /// is, whenever one of its inputs changed. When an input rose the output is that draw. The draw made when
    /// the input falls is used up without effect. The output stays on while an input stays on (observed).
    #[cold]
    #[inline(never)]
    fn draw(&mut self, e: u32) -> bool {
        let draw = self.rng.next_bit();
        draw && self.cell[e as usize].count > 0
    }

    #[cfg_attr(profiling, inline(never))]
    fn step_real(&mut self) {
        self.tick += 1;
        if cfg!(profiling) {
            self.profile.ticks_simulated += 1;
            self.profile.evaluations += self.dirty.len() as u64;
        }
        self.arrays_node = NONE;

        // The components queued for this tick move to `work`; `dirty` (empty, with its capacity) collects the next tick's.
        std::mem::swap(&mut self.dirty, &mut self.work);
        let dirty = std::mem::take(&mut self.work);

        // 1. Evaluate: decide every flip before anything changes.
        let mut flips = std::mem::take(&mut self.flips);
        flips.clear();
        // After a restore the flips of this tick are already known (see `forced`); nothing is queued then.
        flips.append(&mut self.forced);
        for &e in &dirty {
            gm(&mut self.cell, e as usize).queued = false;
            let op = g(&self.cell, e as usize).op;
            // A pin is a latch, except that VMem rejects its input while locked (the input is lost).
            if op == Op::Pin && self.locked {
                gm(&mut self.cell, e as usize).toggle = false;
                continue;
            }
            let next = if op == Op::Random { self.draw(e) } else { self.evaluate(e as usize, op) };
            gm(&mut self.cell, e as usize).toggle = false;
            if next != g(&self.cell, e as usize).on {
                flips.push(e);
            }
        }
        if self.tick == self.clock_due {
            flips.extend(&self.clocks);
            self.clock_due += self.clock_interval;
        }
        if self.tick == self.timer_due {
            flips.extend(&self.timers);
            self.timer_due += self.timer_ticks;
        }
        if self.tick == self.schedule_due {
            flips.extend(&self.schedule.at[self.schedule.key(self.tick)]);
            self.schedule_due = self.schedule.next(self.tick);
        }

        // 2. Apply.
        self.breakpoint_hit = false;
        if cfg!(profiling) {
            self.profile.flips += flips.len() as u64;
        }
        // Which kinds of VMem pin flipped: bit 0 an address pin, bit 1 a content pin.
        // (Bit 2: a Breakpoint, so that the loop does not have to look at what each flipped entity is.)
        let mut pins_flipped = 0u8;
        // Only replay looks at the packed state; a board without replay does not keep it.
        let packing = self.replay.is_some();
        for &e in &flips {
            let on = &mut gm(&mut self.cell, e as usize).on;
            *on = !*on;
            if packing {
                let slot = g(&self.slot, e as usize) as usize;
                *gm(&mut self.packed, slot >> 6) ^= 1 << (slot & 63);
            }
            pins_flipped |= g(&self.pin_role, e as usize);
        }
        if pins_flipped & 4 != 0 {
            self.breakpoint_hit = flips.iter().any(|e| self.cell[*e as usize].op == Op::Breakpoint && self.cell[*e as usize].on);
        }
        if self.split.mode == split::SPLIT {
            self.propagate_split(&flips, true);
        } else {
            self.propagate(&flips);
        }

        // 3. VMem. Observed on the vmem-timing board, tick by tick:
        //   tick T:   address pins flip. A content change in the same tick still goes to the old address.
        //   tick T+1: VMem is locked: input to any pin is rejected and lost. The word at the new address
        //             appears on the content pins in this tick, so readers see it at T+2.
        (self.fetched, self.written, self.vmem_changed) = (None, None, false);
        if self.load_pending || pins_flipped & 3 != 0 || self.locked {
            self.vmem_tick(pins_flipped);
        }
        self.flips = flips;
        self.work = dirty;
        self.work.clear();
        if self.split.mode != split::NORMAL {
            self.mode_tick();
        }
    }

}

impl Engine {
    /// Lowers the memory replay may use (for tests). At the limit v2 keeps running correctly, only slower.
    pub fn set_memory_limit(&mut self, bytes: usize) {
        if let Some(table) = &mut self.replay {
            table.memory_limit = bytes;
        }
    }

    /// What replay does when its table reaches the memory limit: empty it and learn again (true, the default
    /// since v2.1 step 5), or keep it and learn nothing new (false).
    pub fn set_flush_when_full(&mut self, flush: bool) {
        self.flush_when_full = flush;
    }

    /// How often the table was emptied at its limit.
    pub fn replay_flushes(&self) -> u64 {
        self.replay.as_ref().map_or(0, |t| t.flushes)
    }

    /// What a saved replay table must agree on to be usable here: the circuit as this engine holds it, its
    /// settings, and `build` (something that names this build of the engine, since a table is only as right as
    /// the engine that made it).
    fn fingerprint(&self, build: &[u8]) -> [u8; 32] {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(build);
        let mut numbers = |list: &[u32]| {
            h.update((list.len() as u64).to_le_bytes());
            for x in list {
                h.update(x.to_le_bytes());
            }
        };
        numbers(&self.cell.iter().map(|c| c.op as u32).collect::<Vec<_>>());
        for lists in [&self.reads, &self.writes, &self.readers, &self.writers, &self.direct] {
            numbers(&lists.start);
            numbers(&lists.items);
        }
        numbers(&self.live_list);
        numbers(&self.clocks);
        numbers(&self.timers);
        numbers(&self.components.address_pins);
        numbers(&self.components.content_pins);
        for at in &self.schedule.at {
            numbers(at);
        }
        numbers(&[self.clock_interval as u32, self.timer_ticks as u32, self.schedule.lead as u32, self.schedule.period as u32, self.vmem.len() as u32]);
        h.finalize().into()
    }

    /// Starts replay from a table an earlier run saved with `save_replay` (v2.1 step 5). Call it before the
    /// first tick. Returns a line for the user: what was loaded, or why the file was not used.
    pub fn load_replay(&mut self, path: &std::path::Path, build: &[u8]) -> Result<String> {
        let Some(old) = &self.replay else { return Ok("replay is off on this board (Random ink or a simulated Timer): no table to load".into()) };
        if !path.exists() {
            return Ok(format!("{}: not there yet; it will be written at the end", path.display()));
        }
        let fingerprint = self.fingerprint(build);
        let words = self.packed.len() - 1;
        match Table::load(path, &fingerprint, words, old.memory_limit, self.cell.len() as u32, self.vmem.len() as u32)? {
            Ok(table) => {
                let line = format!("{}: {} states loaded ({} MiB)", path.display(), table.nodes(), table.memory() >> 20);
                self.replay = Some(table);
                Ok(line)
            }
            Err(why) => Ok(format!("{}: not used, {why}; it will be replaced at the end", path.display())),
        }
    }

    /// Writes the replay table for a later run. Returns a line for the user.
    pub fn save_replay(&self, path: &std::path::Path, build: &[u8]) -> Result<String> {
        let Some(table) = &self.replay else { return Ok("replay is off on this board: nothing to save".into()) };
        if !table.changed && path.exists() {
            return Ok(format!("{}: nothing new learned, left as it is", path.display()));
        }
        table.save(path, &self.fingerprint(build)).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(format!("{}: {} states saved ({} MiB)", path.display(), table.nodes(), table.memory() >> 20))
    }

    /// (nodes stored, bytes used, ticks crossed by replay)
    pub fn replay_stats(&self) -> (usize, usize, u64) {
        self.replay.as_ref().map_or((0, 0, 0), |t| (t.nodes(), t.memory(), t.replayed_ticks))
    }

    /// Makes `state()` true for the current tick. Replay moves on without the circuit; this brings it along.
    pub fn sync(&mut self) {
        self.leave_mode();
        self.settle();
        self.show();
    }

    /// Puts the circuit where the tick counter is, if replay has run ahead of it.
    #[inline(always)]
    fn settle(&mut self) {
        if !self.real {
            self.restore(self.node);
        }
    }

    /// Copies the signals to where `state()` shows them.
    fn show(&mut self) {
        for (shown, cell) in self.view.iter_mut().zip(&self.cell) {
            *shown = cell.on;
        }
    }

    /// Works out when the clocks, the timers and the schedule flip next, from the tick counter.
    fn set_due(&mut self) {
        let next = |any: bool, interval: u64| if any { (self.tick / interval + 1) * interval } else { u64::MAX };
        self.clock_due = next(!self.clocks.is_empty(), self.clock_interval);
        self.timer_due = next(!self.timers.is_empty(), self.timer_ticks);
        self.schedule_due = self.schedule.next(self.tick);
    }

    fn meta(&self) -> Meta {
        Meta { address: self.address as u32, locked: self.locked, load_pending: self.load_pending, clock_phase: if self.clocks.is_empty() { 0 } else { (self.tick % self.clock_interval) as u32 }, timer_phase: if self.schedule.at.is_empty() { 0 } else { self.schedule.key(self.tick) as u32 } } // the second is the position in the schedule: there is no node on a board with a simulated timer
    }

    /// The node of the current state.
    #[cfg_attr(profiling, inline(never))]
    fn intern(&mut self) -> u32 {
        if cfg!(profiling) {
            self.profile.interns += 1;
        }
        let meta = self.meta();
        // What is pending is stored as its outcome: the components that will flip in the next tick. That is
        // all the queue and the just-risen marks are for, and it is known now, since the next tick evaluates
        // against the signals as they are. (Without Random ink the order of evaluation changes nothing.)
        let mut pending = std::mem::take(&mut self.pending);
        pending.clear();
        for &e in if self.forced.is_empty() { &self.dirty } else { &self.forced } {
            let op = self.cell[e as usize].op;
            if !self.forced.is_empty() || (!(op == Op::Pin && self.locked) && self.evaluate(e as usize, op) != self.cell[e as usize].on) {
                pending.push(e);
            }
        }
        pending.sort_unstable();
        let words = self.packed.len() - 1;
        let table = self.replay.as_mut().unwrap();
        let mut id = table.intern(&self.packed[..words], &pending, &[], meta);
        if id == NONE && self.flush_when_full && table.nodes() > 0 {
            // The table is full and this state is new: forget everything and start learning again from here.
            table.clear();
            self.node = NONE;
            id = table.intern(&self.packed[..words], &pending, &[], meta);
        }
        self.pending = pending;
        self.arrays_node = id;
        id
    }

    /// Puts the circuit into the state of a node. The tick counter is already there.
    #[cfg_attr(profiling, inline(never))]
    fn restore(&mut self, id: u32) {
        if cfg!(profiling) {
            self.profile.restores += 1;
        }
        let table = self.replay.as_ref().unwrap();
        let (bits, dirty, rose, meta) = table.node(id);
        let (bits, pending, _, meta) = (bits, dirty, rose, meta);
        for e in self.dirty.drain(..) {
            self.cell[e as usize].queued = false;
            self.cell[e as usize].toggle = false;
        }
        self.forced.clear();
        self.forced.extend_from_slice(pending);
        if self.arrays_node == NONE {
            for (i, e) in self.live_list.iter().enumerate() {
                self.cell[*e as usize].on = bits[i >> 6] >> (i & 63) & 1 == 1;
            }
            // The counts follow from the signals: a trace counts its writers that are on, a component the traces it reads.
            for &e in &self.live_list {
                let inputs = if self.cell[e as usize].op == Op::Trace { self.writers.of(e as usize) } else { self.reads.of(e as usize) };
                self.cell[e as usize].count = inputs.iter().filter(|i| self.cell[**i as usize].on).count() as u16;
            }
        } else {
            // The arrays hold another node: flip only the signals that differ, and adjust the counts they feed.
            let (old, ..) = table.node(self.arrays_node);
            for (w, (a, b)) in old.iter().zip(bits).enumerate() {
                let mut differ = a ^ b;
                while differ != 0 {
                    let e = self.live_list[w << 6 | differ.trailing_zeros() as usize] as usize;
                    differ &= differ - 1;
                    self.cell[e].on = !self.cell[e].on;
                    let step = if self.cell[e].on { 1 } else { u16::MAX };
                    // A trace feeds its readers; a component feeds the traces it shares and the readers of the traces it owns.
                    let fed = if self.cell[e].op == Op::Trace { [self.readers.of(e), &[]] } else { [self.writes.of(e), self.direct.of(e)] };
                    for &fed in fed.into_iter().flatten() {
                        self.cell[fed as usize].count = self.cell[fed as usize].count.wrapping_add(step);
                    }
                }
            }
        }
        (self.address, self.locked, self.load_pending) = (meta.address as usize, meta.locked, meta.load_pending);
        debug_assert!(self.clocks.is_empty() || meta.clock_phase as u64 == self.tick % self.clock_interval);
        let words = self.packed.len() - 1;
        self.packed[..words].copy_from_slice(self.replay.as_ref().unwrap().node(id).0);
        self.arrays_node = id;
        self.real = true;
        self.set_due(); // replay moved the tick counter
    }
}

impl Simulation for Engine {
    /// One tick, whatever replay knows. Used by tests and tools; `advance` is the fast way.
    fn step(&mut self) {
        self.leave_mode();
        self.split.at_event = false;
        self.settle();
        self.node = NONE;
        self.step_real();
        // `step` is the tick-by-tick way in for tests and tools, which look at `state()` after every tick.
        self.show();
    }

    /// Replay (attempt 8): at a node whose edge and arrival are known, the engine goes to the next event without
    /// the circuit. Otherwise it simulates, and at every event it finds or stores the node it has reached.
    /// Idle-tick jumps (attempt 7): while nothing is queued, no trace is still marked as risen and VMem is at
    /// rest, a tick changes nothing but the counter, so the engine goes straight to the tick of the next
    /// clock or timer flip (or to `until`) and steps there.
    fn advance(&mut self, until: u64) {
        loop {
            if self.node != NONE && self.node_tick == self.tick {
                let table = self.replay.as_mut().unwrap();
                // The tick at which the timers flip next. A step may be replayed only if it ends before that.
                let timer_flip = if self.timers.is_empty() { u64::MAX } else { (self.tick / self.timer_ticks + 1) * self.timer_ticks };
                if let Some(edge) = table.edge(self.node).filter(|edge| self.tick + edge.ticks <= until && self.tick + edge.ticks < timer_flip) {
                    let word = if edge.fetch == NONE { 0 } else { self.vmem[edge.fetch as usize] };
                    if let Some(arrival) = table.arrival(self.node, edge, word) {
                        table.replayed_ticks += edge.ticks;
                        if cfg!(profiling) {
                            self.profile.replay_steps += 1;
                        }
                        self.tick += edge.ticks;
                        self.breakpoint_hit = edge.breakpoint;
                        self.fetched = (edge.fetch != NONE).then_some(edge.fetch as usize);
                        self.written = (edge.write != NONE).then_some(edge.write as usize);
                        self.vmem_changed = false;
                        if edge.write != NONE {
                            self.vmem_changed = self.vmem[edge.write as usize] != arrival.written;
                            self.vmem[edge.write as usize] = arrival.written;
                        }
                        (self.node, self.node_tick, self.real) = (arrival.node, self.tick, false);
                        self.split.at_event = self.split.enabled;
                        return;
                    }
                }
            }
            self.settle();
            if self.split.at_event {
                self.begin_step();
            }
            if self.dirty.is_empty() && self.forced.is_empty() && !self.load_pending && !self.locked {
                let target = until.min(self.clock_due).min(self.timer_due).min(self.schedule_due).min(self.split.due);
                if target > self.tick + 1 {
                    if cfg!(profiling) {
                        self.profile.ticks_idle += target - 1 - self.tick;
                    }
                    self.tick = target - 1;
                }
            }
            self.step_real();
            let event = self.breakpoint_hit || self.fetched.is_some() || self.written.is_some();
            if event && self.replay.is_some() {
                let reached = self.intern();
                // A step in which a timer flipped depends on when it flipped: it is not recorded.
                let timer_flipped = !self.timers.is_empty() && self.tick / self.timer_ticks != self.node_tick / self.timer_ticks;
                if self.node != NONE && reached != NONE && !timer_flipped {
                    let address = |a: Option<usize>| a.map_or(NONE, |a| a as u32);
                    let edge = Edge { ticks: self.tick - self.node_tick, fetch: address(self.fetched), write: address(self.written), breakpoint: self.breakpoint_hit };
                    let written = self.written.map_or(0, |a| self.vmem[a]);
                    self.replay.as_mut().unwrap().record(self.node, edge, self.loaded_word, Arrival { node: reached, written });
                }
                (self.node, self.node_tick) = (reached, self.tick);
            }
            if event || self.tick >= until {
                self.split.at_event = event && self.split.enabled;
                return;
            }
        }
    }

    fn tick(&self) -> u64 {
        self.tick
    }

    fn state(&self) -> &[bool] {
        &self.view
    }

    fn breakpoint_hit(&self) -> bool {
        self.breakpoint_hit
    }

    fn vmem(&self) -> &[u32] {
        &self.vmem
    }

    fn vmem_access(&self) -> (Option<usize>, Option<usize>) {
        (self.fetched, self.written)
    }

    fn vmem_changed(&self) -> bool {
        self.vmem_changed
    }
}
