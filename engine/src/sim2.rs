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

/// Many short lists in one array (attempt 3): the list of entity e is `items[start[e]..start[e + 1]]`.
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
        &self.items[self.start[e] as usize..self.start[e + 1] as usize]
    }
}

/// The part of a board that runs by itself (attempt 11): everything a clock drives that no VMem pin, Random or
/// Timer can ever influence. Its behaviour is fixed from tick 0 and soon repeats, so it is run once, before
/// the simulation, and then only the flips it hands to the rest of the circuit are played from this schedule.
#[derive(Default)]
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
                for k in (1..last).rev() {
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
    /// A member of a unit that is being replayed (part-level skipping): its signals are not kept up to date,
    /// nothing wakes it, and what the rest of the circuit needs from the unit comes from the unit's table.
    Frozen,
}

pub struct Engine {
    pub components: Components,
    ops: Vec<Op>,
    reads: Lists,
    writes: Lists,
    readers: Lists,
    writers: Lists,
    /// The self-running part's flips (attempt 11). Empty when the board has none.
    schedule: Schedule,
    /// Per component or pin: the readers of the traces that only it writes. Such a trace always equals its
    /// writer, so it is not simulated: its readers read the writer itself (attempt 10).
    direct: Lists,
    /// Per component: how many of the traces it reads are on. Per trace: how many of its writers are on.
    /// Kept up to date when a trace or a writer changes, so nothing is counted when a component is evaluated (attempt 4).
    count: Vec<u32>,
    /// Per entity: whether it is simulated: it can reach a VMem pin, a Breakpoint or a Random, and it is not a
    /// trace with one writer (which always equals that writer). For the others `state()` is not meaningful.
    live: Vec<bool>,
    /// Per gate whose output depends only on a count passing one value: that value (1 for OR and NOR, the
    /// number of inputs for AND and NAND). Such a gate is woken only when its count passes it (attempt 5).
    threshold: Vec<u32>,
    /// Per entity: a trace is on when any writer is on; a component's value is its output.
    on: Vec<bool>,
    /// Per trace: it went from off to on in the most recent Apply step.
    rose: Vec<bool>,
    rose_list: Vec<u32>,
    /// Components to evaluate on the next tick.
    dirty: Vec<u32>,
    /// The two lists a tick works on, kept between ticks so that a tick allocates nothing (attempt 1).
    work: Vec<u32>,
    flips: Vec<u32>,
    queued: Vec<bool>,
    clocks: Vec<u32>,
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
    /// Part-level skipping (experimental, off unless `enable_units` is called). See `Unit`.
    units: Vec<Unit>,
    /// Per entity: its unit (NO_UNIT: none), and the units that read it from outside.
    unit_of: Vec<u16>,
    feeds: Lists,
    /// The operation of every entity when it is not frozen.
    base_ops: Vec<Op>,
    /// Units that need a look at the end of the tick: one of their inputs flipped, or they are in a burst.
    touched: Vec<u16>,
    active: Vec<u16>,
    scratch: Vec<u64>,
    /// What VMem did in the last tick: the address loaded from, the address written to, and whether a word changed.
    fetched: Option<usize>,
    written: Option<usize>,
    vmem_changed: bool,
    /// Per entity: 1 for an address pin, 2 for a content pin, 0 otherwise.
    pin_role: Vec<u8>,
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
        // Replay is off with Random ink (each evaluation uses up a draw) and with a simulated Timer (its position
        // within its interval would be part of every state, so states would hardly ever repeat).
        let timed = (0..n).any(|e| live[e] && components.kinds[e] == Kind::Component(Ink::Timer));
        let replay = (!ops.contains(&Op::Random) && !timed).then(|| Table::new(live_list.len().div_ceil(64), MEMORY_LIMIT));
        let mut engine = Engine { loaded_word: 0, replay, scratch: vec![0; live_list.len().div_ceil(64)], live_list, node: NONE, node_tick: 0, real: true, arrays_node: NONE, forced: Vec::new(), pending: Vec::new(), units: Vec::new(), unit_of: Vec::new(), feeds: Lists::new(&[]), base_ops: Vec::new(), touched: Vec::new(), active: Vec::new(), schedule, ops, reads, writes, readers, writers, direct, count: vec![0; n], live, threshold: vec![0; n], on: vec![false; n], rose: vec![false; n], rose_list: Vec::new(), dirty: Vec::new(), work: Vec::new(), flips: Vec::new(), queued: vec![false; n], clocks: Vec::new(), clock_interval, timers: Vec::new(), timer_ticks, rng, tick: 0, breakpoint_hit: false, vmem: board.vmem.clone(), address: 0, locked: false, load_pending: false, fetched: None, written: None, vmem_changed: false, pin_role: vec![0; n], components };
        for (role, pins) in [(1, &engine.components.address_pins), (2, &engine.components.content_pins)] {
            for pin in pins {
                engine.pin_role[*pin as usize] = role;
            }
        }
        let c = &engine.components;
        // Observed (gates board): at tick 0 every component already has the output it gets with all
        // inputs off, so Latch On and the inverting gates start on.
        for (e, kind) in c.kinds.iter().enumerate() {
            if engine.ops[e] == Op::Dead {
                continue;
            }
            match kind {
                // Observed (general boards): a clock is on at tick 0 and flips at every multiple of the interval.
                Kind::Component(Ink::Clock) => {
                    if !keep[e] {
                        engine.clocks.push(e as u32);
                    }
                    engine.on[e] = true;
                }
                // Observed (random-pulse board): unlike a clock, a timer starts off.
                Kind::Component(Ink::Timer) => engine.timers.push(e as u32),
                Kind::Component(Ink::LatchOn) => engine.on[e] = true,
                // (Written out, not evaluated: a component on the schedule has no operation to evaluate.)
                Kind::Component(Ink::Not | Ink::Nor | Ink::Xnor) => engine.on[e] = true,
                Kind::Component(Ink::Nand) => engine.on[e] = !c.reads[e].is_empty(),
                _ => {}
            }
        }
        // Their traces are on at tick 0 too, and count as having just risen: a latch reading one toggles at tick 1.
        for e in 0..n {
            if c.kinds[e] == Kind::Trace && c.writers[e].iter().any(|w| engine.on[*w as usize]) {
                engine.on[e] = true;
                engine.rose[e] = true;
                engine.rose_list.push(e as u32);
                for &reader in &c.readers[e] {
                    if !engine.queued[reader as usize] && engine.ops[reader as usize] != Op::Dead {
                        engine.queued[reader as usize] = true;
                        engine.dirty.push(reader);
                    }
                }
            }
        }
        // A merged trace that is on at tick 0 has risen: its readers look for that at its writer.
        let risen = std::mem::take(&mut engine.rose_list);
        for t in risen {
            let e = if merged[t as usize] { engine.writers.of(t as usize)[0] } else { t };
            engine.rose[t as usize] = false;
            if !std::mem::replace(&mut engine.rose[e as usize], true) {
                engine.rose_list.push(e);
            }
        }
        for e in 0..n {
            let inputs = if engine.ops[e] == Op::Trace { engine.writers.of(e) } else { engine.reads.of(e) };
            engine.count[e] = inputs.iter().filter(|i| engine.on[**i as usize]).count() as u32;
            engine.threshold[e] = if matches!(engine.ops[e], Op::And | Op::Nand) { inputs.len() as u32 } else { 1 };
        }
        Ok(engine)
    }

    /// The output a component would have after this tick, from the trace states of the previous tick.
    #[inline(always)]
    fn evaluate(&self, e: usize, op: Op) -> bool {
        let (active, inputs) = (self.count[e], self.reads.start[e + 1] - self.reads.start[e]);
        match op {
            Op::Or | Op::Breakpoint => active > 0,
            Op::Nor => active == 0,
            Op::And => active > 0 && active == inputs,
            // Observed: a NAND with no inputs is off, so this is not the exact inverse of AND.
            Op::Nand => active < inputs,
            Op::Xor => active % 2 == 1,
            Op::Xnor => active % 2 == 0,
            // Observed: toggles once per input that rose, so two inputs rising together cancel out.
            Op::Latch | Op::Pin => self.on[e] != (self.reads.of(e).iter().filter(|t| self.rose[**t as usize]).count() % 2 == 1),
            // Clocks and timers flip from the schedule in `step`, never from their inputs (observed).
            Op::Scheduled => self.on[e],
            Op::Random | Op::Trace | Op::Dead | Op::Frozen => unreachable!("not evaluated here"),
        }
    }
}

impl Engine {
    /// Per entity: whether v2 simulates it. `state()` is only meaningful for these.
    pub fn live(&self) -> &[bool] {
        &self.live
    }
}

impl Engine {
    /// The number held by a group of pins, bit 0 first.
    fn pins_value(&self, pins: &[u32]) -> u32 {
        pins.iter().enumerate().map(|(k, pin)| (self.on[*pin as usize] as u32) << k).sum()
    }

    /// After components or pins flipped: recompute the traces they write, mark the ones that rose,
    /// and queue the readers of every trace that changed for the next tick.
    fn propagate(&mut self, flipped: &[u32]) {
        // First every writer count, then the traces: a trace that loses one writer and gains another in the
        // same tick does not change, exactly as when all its writers are looked at afterwards.
        for &e in flipped {
            let step = if self.on[e as usize] { 1 } else { u32::MAX }; // +1 or -1
            for &t in self.writes.of(e as usize) {
                self.count[t as usize] = self.count[t as usize].wrapping_add(step);
            }
        }
        for &e in flipped {
            // The traces only this component writes: their readers see the component itself.
            let (from, to) = (self.direct.start[e as usize] as usize, self.direct.start[e as usize + 1] as usize);
            if from != to {
                let now = self.on[e as usize];
                if now {
                    self.rose[e as usize] = true;
                    self.rose_list.push(e);
                }
                for i in from..to {
                    self.feed(self.direct.items[i], now);
                }
            }
            let (from, to) = (self.writes.start[e as usize] as usize, self.writes.start[e as usize + 1] as usize);
            for i in from..to {
                let t = self.writes.items[i];
                if self.ops[t as usize] == Op::Frozen {
                    continue;
                }
                let now = self.count[t as usize] > 0;
                if now == self.on[t as usize] {
                    continue;
                }
                self.on[t as usize] = now;
                self.note_source(t);
                if now {
                    self.rose[t as usize] = true;
                    self.rose_list.push(t);
                }
                for k in self.readers.start[t as usize] as usize..self.readers.start[t as usize + 1] as usize {
                    self.feed(self.readers.items[k], now);
                }
            }
        }
    }

    /// One input of `reader` turned on (`now`) or off: adjusts its count and queues it if its output can change.
    #[inline(always)]
    fn feed(&mut self, reader: u32, now: bool) {
        if self.ops[reader as usize] == Op::Frozen {
            return;
        }
        let old = self.count[reader as usize];
        let new = old.wrapping_add(if now { 1 } else { u32::MAX });
        self.count[reader as usize] = new;
        // A gate that is not queued has the output its count gives. It can only change when the count
        // passes its threshold: the higher of the two counts is then the threshold.
        let wake = match self.ops[reader as usize] {
            Op::Or | Op::Breakpoint | Op::Nor | Op::And | Op::Nand => old.max(new) == self.threshold[reader as usize],
            Op::Xor | Op::Xnor | Op::Random => true, // a Random draws at every change of an input
            Op::Latch | Op::Pin => now,              // only a rising input toggles a latch
            Op::Scheduled | Op::Trace | Op::Dead | Op::Frozen => false,
        };
        if wake && !self.queued[reader as usize] {
            self.queued[reader as usize] = true;
            self.dirty.push(reader);
        }
    }
}

impl Engine {
    /// One tick of the circuit itself.
    fn step_real(&mut self) {
        self.tick += 1;
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
            self.queued[e as usize] = false;
            let op = self.ops[e as usize];
            // A pin is a latch, except that VMem rejects its input while locked (the input is lost).
            if op == Op::Pin && self.locked {
                continue;
            }
            let next = if op == Op::Random {
                // Observed (random recorder boards): a Random draws every time it is evaluated, that is, whenever
                // one of its inputs changed. When an input rose the output is that draw. The draw made when the
                // input falls is used up without effect. The output stays on while an input stays on (observed).
                let draw = self.rng.next_bit();
                draw && self.count[e as usize] > 0
            } else {
                self.evaluate(e as usize, op)
            };
            if next != self.on[e as usize] {
                flips.push(e);
            }
        }
        if self.tick % self.clock_interval == 0 {
            flips.extend(&self.clocks);
        }
        if self.tick % self.timer_ticks == 0 {
            flips.extend(&self.timers);
        }
        if !self.schedule.at.is_empty() {
            flips.extend(&self.schedule.at[self.schedule.key(self.tick)]);
        }
        // A unit that is being replayed in the middle of a burst: its table says what it hands out in this tick.
        for &u in &self.active {
            let unit = &self.units[u as usize];
            if unit.frozen {
                flips.extend(unit.outs(unit.node));
            }
        }

        // 2. Apply.
        for t in self.rose_list.drain(..) {
            self.rose[t as usize] = false;
        }
        self.breakpoint_hit = false;
        for &e in &flips {
            self.on[e as usize] = !self.on[e as usize];
            if self.ops[e as usize] == Op::Breakpoint && self.on[e as usize] {
                self.breakpoint_hit = true;
            }
        }
        if !self.units.is_empty() {
            for &e in &flips {
                self.note_source(e);
            }
        }
        self.propagate(&flips);

        // 3. VMem. Observed on the vmem-timing board, tick by tick:
        //   tick T:   address pins flip. A content change in the same tick still goes to the old address.
        //   tick T+1: VMem is locked: input to any pin is rejected and lost. The word at the new address
        //             appears on the content pins in this tick, so readers see it at T+2.
        (self.fetched, self.written, self.vmem_changed) = (None, None, false);
        if !self.components.address_pins.is_empty() || !self.components.content_pins.is_empty() {
            if self.load_pending {
                self.load_pending = false;
                self.fetched = Some(self.address);
                let word = self.vmem[self.address];
                self.loaded_word = word;
                let mut loaded = Vec::new();
                for (k, &pin) in self.components.content_pins.iter().enumerate() {
                    if self.on[pin as usize] != (word >> k & 1 == 1) {
                        self.on[pin as usize] = !self.on[pin as usize];
                        loaded.push(pin);
                    }
                }
                for &e in &loaded {
                    self.note_source(e);
                }
                self.propagate(&loaded);
            }
            let flipped = |role| flips.iter().any(|e| self.pin_role[*e as usize] == role);
            let (address_flipped, content_flipped) = (flipped(1), flipped(2));
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
        self.flips = flips;
        self.work = dirty;
        self.work.clear();
        if !self.touched.is_empty() || !self.active.is_empty() {
            self.units_end_of_tick();
        }
        if std::env::var_os("VCB_UNITS_CHECK").is_some() {
            self.units_check();
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

    /// (nodes stored, bytes used, ticks crossed by replay)
    pub fn replay_stats(&self) -> (usize, usize, u64) {
        self.replay.as_ref().map_or((0, 0, 0), |t| (t.nodes(), t.memory(), t.replayed_ticks))
    }

    /// Makes `state()` true for the current tick. Replay moves on without the circuit; this brings it along.
    pub fn sync(&mut self) {
        for u in 0..self.units.len() {
            if self.units[u].frozen {
                self.unit_thaw(u, false);
            }
        }
        self.sync_board();
    }

    /// The whole-board half of `sync`: frozen units stay frozen.
    fn sync_board(&mut self) {
        if !self.real {
            self.restore(self.node);
        }
    }

    fn meta(&self) -> Meta {
        Meta { address: self.address as u32, locked: self.locked, load_pending: self.load_pending, clock_phase: if self.clocks.is_empty() { 0 } else { (self.tick % self.clock_interval) as u32 }, timer_phase: if self.schedule.at.is_empty() { 0 } else { self.schedule.key(self.tick) as u32 } } // the second is the position in the schedule: there is no node on a board with a simulated timer
    }

    /// The node of the current state.
    fn intern(&mut self) -> u32 {
        let mut scratch = std::mem::take(&mut self.scratch);
        // One word at a time, built in a register: 64 independent loads, one store.
        for (word, entities) in scratch.iter_mut().zip(self.live_list.chunks(64)) {
            let mut bits = 0u64;
            for (i, e) in entities.iter().enumerate() {
                bits |= (self.on[*e as usize] as u64) << i;
            }
            *word = bits;
        }
        let meta = self.meta();
        // What is pending is stored as its outcome: the components that will flip in the next tick. That is
        // all the queue and the just-risen marks are for, and it is known now, since the next tick evaluates
        // against the signals as they are. (Without Random ink the order of evaluation changes nothing.)
        let mut pending = std::mem::take(&mut self.pending);
        pending.clear();
        for &e in if self.forced.is_empty() { &self.dirty } else { &self.forced } {
            let op = self.ops[e as usize];
            if !self.forced.is_empty() || (!(op == Op::Pin && self.locked) && self.evaluate(e as usize, op) != self.on[e as usize]) {
                pending.push(e);
            }
        }
        pending.sort_unstable();
        let id = self.replay.as_mut().unwrap().intern(&scratch, &pending, &[], meta);
        self.pending = pending;
        self.scratch = scratch;
        self.arrays_node = id;
        id
    }

    /// Puts the circuit into the state of a node. The tick counter is already there.
    fn restore(&mut self, id: u32) {
        let table = self.replay.as_ref().unwrap();
        let (bits, dirty, rose, meta) = table.node(id);
        let (bits, pending, _, meta) = (bits, dirty, rose, meta);
        for e in self.dirty.drain(..) {
            self.queued[e as usize] = false;
        }
        for t in self.rose_list.drain(..) {
            self.rose[t as usize] = false;
        }
        self.forced.clear();
        self.forced.extend_from_slice(pending);
        if self.arrays_node == NONE {
            for (i, e) in self.live_list.iter().enumerate() {
                self.on[*e as usize] = bits[i >> 6] >> (i & 63) & 1 == 1;
            }
            // The counts follow from the signals: a trace counts its writers that are on, a component the traces it reads.
            for &e in &self.live_list {
                let inputs = if self.ops[e as usize] == Op::Trace { self.writers.of(e as usize) } else { self.reads.of(e as usize) };
                self.count[e as usize] = inputs.iter().filter(|i| self.on[**i as usize]).count() as u32;
            }
        } else {
            // The arrays hold another node: flip only the signals that differ, and adjust the counts they feed.
            let (old, ..) = table.node(self.arrays_node);
            for (w, (a, b)) in old.iter().zip(bits).enumerate() {
                let mut differ = a ^ b;
                while differ != 0 {
                    let e = self.live_list[w << 6 | differ.trailing_zeros() as usize] as usize;
                    differ &= differ - 1;
                    self.on[e] = !self.on[e];
                    let step = if self.on[e] { 1 } else { u32::MAX };
                    // A trace feeds its readers; a component feeds the traces it shares and the readers of the traces it owns.
                    let fed = if self.ops[e] == Op::Trace { [self.readers.of(e), &[]] } else { [self.writes.of(e), self.direct.of(e)] };
                    for &fed in fed.into_iter().flatten() {
                        self.count[fed as usize] = self.count[fed as usize].wrapping_add(step);
                    }
                }
            }
        }
        (self.address, self.locked, self.load_pending) = (meta.address as usize, meta.locked, meta.load_pending);
        debug_assert!(self.clocks.is_empty() || meta.clock_phase as u64 == self.tick % self.clock_interval);
        self.arrays_node = id;
        self.real = true;
    }
}

impl Simulation for Engine {
    /// One tick, whatever replay knows. Used by tests and tools; `advance` is the fast way.
    fn step(&mut self) {
        self.sync();
        self.node = NONE;
        self.step_real();
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
                if let Some(edge) = table.edge(self.node).filter(|edge| self.tick + edge.ticks <= until) {
                    let word = if edge.fetch == NONE { 0 } else { self.vmem[edge.fetch as usize] };
                    if let Some(arrival) = table.arrival(self.node, edge, word) {
                        table.replayed_ticks += edge.ticks;
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
                        return;
                    }
                }
            }
            self.sync_board();
            if self.dirty.is_empty() && self.forced.is_empty() && self.rose_list.is_empty() && !self.load_pending && !self.locked && self.active.is_empty() {
                let next_flip = |ticks: &[u32], interval: u64| if ticks.is_empty() { u64::MAX } else { (self.tick / interval + 1) * interval };
                let target = until.min(next_flip(&self.clocks, self.clock_interval)).min(next_flip(&self.timers, self.timer_ticks)).min(self.schedule.next(self.tick));
                if target > self.tick + 1 {
                    self.tick = target - 1;
                }
            }
            self.step_real();
            let event = self.breakpoint_hit || self.fetched.is_some() || self.written.is_some();
            if event && self.replay.is_some() {
                let reached = self.intern();
                if self.node != NONE && reached != NONE {
                    let address = |a: Option<usize>| a.map_or(NONE, |a| a as u32);
                    let edge = Edge { ticks: self.tick - self.node_tick, fetch: address(self.fetched), write: address(self.written), breakpoint: self.breakpoint_hit };
                    let written = self.written.map_or(0, |a| self.vmem[a]);
                    self.replay.as_mut().unwrap().record(self.node, edge, self.loaded_word, Arrival { node: reached, written });
                }
                (self.node, self.node_tick) = (reached, self.tick);
            }
            if event || self.tick >= until {
                return;
            }
        }
    }

    fn tick(&self) -> u64 {
        self.tick
    }

    fn state(&self) -> &[bool] {
        &self.on
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

// ---------------------------------------------------------------------------------------------------------
// Part-level skipping (experimental; specs/vcb-engine-v2-log.md, "Ideas not tried" and attempt 12).
//
// A **unit** is a sparsely connected part of the circuit, found from the netlist alone. The rest of the
// circuit steers it only through its inputs, and sees of it only the flips of its output components.
// A unit has its own table of **nodes**: its signals, its inputs' values and the components of it that flip
// in the next tick. From a node, the next node follows from which inputs flip in the tick in between: none
// (the common case inside a burst: one array lookup) or a set of them (one hash lookup). While a unit is
// **frozen** its inside is not simulated at all: each tick its node says which outputs flip, and the next
// node is looked up. When a lookup fails the unit is thawed: its signals are rebuilt from the node and it is
// simulated like everything else, which also teaches the table what it did not know.
// ---------------------------------------------------------------------------------------------------------

const NO_UNIT: u16 = u16::MAX;

struct UnitNode {
    hash: u64,
    /// The next node with the same hash.
    same: u32,
    /// In `lists`: the components that flip in the next tick, then those of them the outside sees.
    at: u32,
    pending: u32,
    outs: u32,
}

#[derive(Default)]
struct Unit {
    /// The gates and latches of the unit, and its traces (those nothing outside reads).
    comps: Vec<u32>,
    traces: Vec<u32>,
    /// What the unit reads from outside.
    inputs: Vec<u32>,
    words: usize,
    bits: Vec<u64>,
    lists: Vec<u32>,
    nodes: Vec<UnitNode>,
    by_hash: std::collections::HashMap<u64, u32, std::hash::BuildHasherDefault<crate::replay::Mix>>,
    /// Per node: the node one tick later when no input flips (NONE: not seen yet).
    next: Vec<u32>,
    /// (node, the inputs that flipped) to the node one tick later.
    by_event: std::collections::HashMap<(u32, u64), u32, std::hash::BuildHasherDefault<crate::replay::Mix>>,
    frozen: bool,
    node: u32,
    /// The inputs that flipped in this tick, as a sum of their hashes (0: none).
    signature: u64,
    touched: bool,
    scratch: Vec<u64>,
    /// Statistics: flips not simulated, lookups that failed, nodes looked up or stored while simulating.
    skipped: u64,
    misses: u64,
    recorded: u64,
}

impl Unit {
    fn pending(&self, node: u32) -> &[u32] {
        let n = &self.nodes[node as usize];
        &self.lists[n.at as usize..(n.at + n.pending) as usize]
    }

    fn outs(&self, node: u32) -> &[u32] {
        let n = &self.nodes[node as usize];
        &self.lists[(n.at + n.pending) as usize..(n.at + n.pending + n.outs) as usize]
    }
}

impl Engine {
    /// An input of some unit flipped: the unit must look at its situation at the end of this tick.
    #[inline(always)]
    fn note_source(&mut self, e: u32) {
        if self.feeds.start.len() <= 1 {
            return;
        }
        for i in self.feeds.start[e as usize] as usize..self.feeds.start[e as usize + 1] as usize {
            let u = self.feeds.items[i] as u16;
            let unit = &mut self.units[u as usize];
            // A well mixed number per input, summed: the order in which inputs flip within a tick does not matter,
            // and two different sets do not meet. (A plain multiple of the entity number would: 246 + 252 = 248 + 250.)
            let mut h = (e as u64 + 1).wrapping_mul(0x9e37_79b9_7f4a_7c15);
            h = (h ^ h >> 30).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            h = (h ^ h >> 27).wrapping_mul(0x94d0_49bb_1331_11eb);
            unit.signature = unit.signature.wrapping_add((h ^ h >> 31) | 1);
            if !std::mem::replace(&mut unit.touched, true) {
                self.touched.push(u);
            }
        }
    }

    /// Switches part-level skipping on (experimental). Call it before the first tick. The parts are found from
    /// the netlist alone: every trace that more than T components share is cut, T = 32, 16, 8, 4, 2; a part is
    /// split while it has more than `largest` entities and becomes a unit if it has at least `smallest`.
    /// Whole-board replay is switched off: the two are not joined yet.
    pub fn enable_units(&mut self, largest: usize, smallest: usize) {
        let c = &self.components;
        let n = c.kinds.len();
        self.replay = None;
        self.base_ops = self.ops.clone();
        if self.ops.contains(&Op::Random) || n == 0 {
            return; // every flip must come in v1's order
        }
        // The same picture of the circuit as the estimator's: what is simulated, with the one-writer traces.
        let simulated: Vec<bool> = (0..n).map(|e| self.live[e] || (c.kinds[e] == Kind::Trace && c.writers[e].len() == 1 && self.live[c.writers[e][0] as usize])).collect();
        let degree: Vec<usize> = (0..n).map(|e| if c.kinds[e] == Kind::Trace && simulated[e] { c.readers[e].iter().chain(&c.writers[e]).filter(|x| simulated[**x as usize]).count() } else { 0 }).collect();
        let connected = |inside: &[bool], threshold: usize| {
            let mut part = vec![NONE; n];
            let mut sizes: Vec<usize> = Vec::new();
            for start in 0..n {
                if !inside[start] || degree[start] > threshold || part[start] != NONE {
                    continue;
                }
                let id = sizes.len() as u32;
                let (mut todo, mut size) = (vec![start], 0);
                part[start] = id;
                while let Some(e) = todo.pop() {
                    size += 1;
                    let next = if c.kinds[e] == Kind::Trace { c.readers[e].iter().chain(&c.writers[e]) } else { c.reads[e].iter().chain(&c.writes[e]) };
                    for &x in next {
                        let x = x as usize;
                        if inside[x] && degree[x] <= threshold && part[x] == NONE {
                            part[x] = id;
                            todo.push(x);
                        }
                    }
                }
                sizes.push(size);
            }
            (part, sizes)
        };
        // From the coarsest cut down: a part that is small enough is taken, a larger one is cut again.
        let mut open = simulated.clone();
        let mut chosen: Vec<Vec<usize>> = Vec::new();
        for threshold in [32usize, 16, 8, 4, 2] {
            let (part, sizes) = connected(&open, threshold);
            let mut groups: Vec<Vec<usize>> = vec![Vec::new(); sizes.len()];
            for e in 0..n {
                if part[e] != NONE {
                    groups[part[e] as usize].push(e);
                }
            }
            for (group, size) in groups.into_iter().zip(sizes) {
                if size <= largest {
                    for &e in &group {
                        open[e] = false;
                    }
                    if size >= smallest {
                        chosen.push(group);
                    }
                }
            }
        }
        // The members: gates and latches, and the traces that only members read. Pins, Breakpoints and what the
        // schedule drives stay outside: something else also moves them, or their flip must be seen.
        self.unit_of = vec![NO_UNIT; n];
        let member = |op: Op| matches!(op, Op::Or | Op::Nor | Op::And | Op::Nand | Op::Xor | Op::Xnor | Op::Latch);
        for group in chosen {
            let u = self.units.len() as u16;
            let comps: Vec<u32> = group.iter().filter(|e| self.live[**e] && member(self.ops[**e])).map(|e| *e as u32).collect();
            if comps.len() < 8 || u == NO_UNIT - 1 {
                continue;
            }
            for &e in &comps {
                self.unit_of[e as usize] = u;
            }
            let traces: Vec<u32> = group.iter().filter(|t| self.live[**t] && self.ops[**t] == Op::Trace && self.readers.of(**t).iter().all(|r| self.unit_of[*r as usize] == u)).map(|t| *t as u32).collect();
            for &t in &traces {
                self.unit_of[t as usize] = u;
            }
            self.units.push(Unit { comps, traces, node: NONE, ..Default::default() });
        }
        // Inputs: what a member reads that is not a member; for a member trace, its writers that are not members.
        let mut feeds: Vec<Vec<u32>> = vec![Vec::new(); n];
        for u in 0..self.units.len() {
            let mut inputs: Vec<u32> = Vec::new();
            for &e in self.units[u].comps.iter() {
                inputs.extend(self.reads.of(e as usize).iter().filter(|x| self.unit_of[**x as usize] != u as u16));
            }
            for &t in self.units[u].traces.iter() {
                inputs.extend(self.writers.of(t as usize).iter().filter(|x| self.unit_of[**x as usize] != u as u16));
            }
            inputs.sort_unstable();
            inputs.dedup();
            for &x in &inputs {
                feeds[x as usize].push(u as u32);
            }
            let unit = &mut self.units[u];
            unit.words = (unit.comps.len() + unit.traces.len() + inputs.len()).div_ceil(64);
            unit.scratch = vec![0; unit.words];
            unit.inputs = inputs;
            // Every unit starts simulated, and looks at its situation after the first tick.
            unit.touched = true;
            self.touched.push(u as u16);
        }
        self.feeds = Lists::new(&feeds);
    }

    /// (units, flips they skipped, lookups that failed, nodes looked up or stored while simulating, nodes kept)
    pub fn unit_stats(&self) -> (usize, u64, u64, u64, usize) {
        self.units.iter().fold((self.units.len(), 0, 0, 0, 0), |a, u| (a.0, a.1 + u.skipped, a.2 + u.misses, a.3 + u.recorded, a.4 + u.nodes.len()))
    }

    /// True when the outside sees component `e` of unit `u` flip: something outside reads it or a trace it writes.
    fn unit_output(&self, u: u16, e: u32) -> bool {
        self.direct.of(e as usize).iter().any(|r| self.unit_of[*r as usize] != u) || self.writes.of(e as usize).iter().any(|t| self.unit_of[*t as usize] != u)
    }

    /// The node of a simulated unit as it stands at the end of a tick. (node, whether it was known already)
    fn unit_intern(&mut self, u: usize) -> (u32, bool) {
        /// Nodes a unit may keep. At the limit it learns nothing new and stays simulated where it does not know.
        const LIMIT: usize = 1 << 20;
        let mut unit = std::mem::take(&mut self.units[u]);
        unit.recorded += 1;
        let mut scratch = std::mem::take(&mut unit.scratch);
        let all = unit.comps.iter().chain(&unit.traces).chain(&unit.inputs);
        scratch.fill(0);
        for (i, e) in all.enumerate() {
            scratch[i >> 6] |= (self.on[*e as usize] as u64) << (i & 63);
        }
        // What flips in the next tick: what was forced, and what is queued and evaluates to a change.
        let mut pending = std::mem::take(&mut self.pending);
        pending.clear();
        pending.extend(self.forced.iter().filter(|e| self.unit_of[**e as usize] == u as u16));
        for &e in &self.dirty {
            if self.unit_of[e as usize] == u as u16 && self.evaluate(e as usize, self.ops[e as usize]) != self.on[e as usize] {
                pending.push(e);
            }
        }
        pending.sort_unstable();
        pending.dedup();
        let mut hash = 0x243f_6a88_85a3_08d3u64;
        for w in scratch.iter().chain(pending.iter().map(|e| *e as u64 | 1 << 40).collect::<Vec<u64>>().iter()) {
            hash = (hash ^ *w).wrapping_mul(0x9e37_79b9_7f4a_7c15).rotate_left(31);
        }
        let mut id = unit.by_hash.get(&hash).copied().unwrap_or(NONE);
        let first = id;
        while id != NONE {
            if unit.bits[id as usize * unit.words..(id as usize + 1) * unit.words] == scratch[..] && unit.pending(id) == &pending[..] {
                break;
            }
            id = unit.nodes[id as usize].same;
        }
        let known = id != NONE;
        if !known && unit.nodes.len() < LIMIT {
            id = unit.nodes.len() as u32;
            let outs: Vec<u32> = pending.iter().copied().filter(|e| self.unit_output(u as u16, *e)).collect();
            unit.nodes.push(UnitNode { hash, same: first, at: unit.lists.len() as u32, pending: pending.len() as u32, outs: outs.len() as u32 });
            unit.lists.extend(&pending);
            unit.lists.extend(&outs);
            unit.bits.extend(&scratch);
            unit.next.push(NONE);
            unit.by_hash.insert(hash, id);
        }
        unit.scratch = scratch;
        self.pending = pending;
        self.units[u] = unit;
        (id, known)
    }

    /// Stops simulating a unit: it stands at a node it knows.
    fn unit_freeze(&mut self, u: usize) {
        let unit = std::mem::take(&mut self.units[u]);
        for &e in unit.comps.iter().chain(&unit.traces) {
            self.ops[e as usize] = Op::Frozen;
        }
        let (unit_of, queued) = (&self.unit_of, &mut self.queued);
        self.dirty.retain(|e| {
            let inside = unit_of[*e as usize] == u as u16;
            if inside {
                queued[*e as usize] = false;
            }
            !inside
        });
        self.units[u] = Unit { frozen: true, ..unit };
    }

    /// Makes a frozen unit simulated again, from its node. `stepped`: the tick after that node has already
    /// happened for the rest of the circuit (a lookup failed), so the unit catches up with it here: its
    /// pending flips are applied and every member is evaluated in the next tick. Otherwise the unit stands at
    /// its node and its pending flips are left for the next tick.
    fn unit_thaw(&mut self, u: usize, stepped: bool) {
        let unit = std::mem::take(&mut self.units[u]);
        let bits = &unit.bits[unit.node as usize * unit.words..(unit.node as usize + 1) * unit.words];
        for (i, &e) in unit.comps.iter().chain(&unit.traces).enumerate() {
            self.ops[e as usize] = self.base_ops[e as usize];
            self.on[e as usize] = bits[i >> 6] >> (i & 63) & 1 == 1;
        }
        if stepped {
            for &e in unit.pending(unit.node) {
                self.on[e as usize] = !self.on[e as usize];
                if self.on[e as usize] && !std::mem::replace(&mut self.rose[e as usize], true) {
                    self.rose_list.push(e);
                }
            }
        } else {
            self.forced.extend(unit.pending(unit.node));
        }
        // The counts follow from the signals. A trace of the unit is on when one of its writers is.
        for &t in &unit.traces {
            let count = self.writers.of(t as usize).iter().filter(|w| self.on[**w as usize]).count() as u32;
            self.count[t as usize] = count;
            if stepped && (count > 0) != self.on[t as usize] {
                self.on[t as usize] = count > 0;
                if count > 0 && !std::mem::replace(&mut self.rose[t as usize], true) {
                    self.rose_list.push(t);
                }
            }
        }
        for &e in &unit.comps {
            self.count[e as usize] = self.reads.of(e as usize).iter().filter(|i| self.on[**i as usize]).count() as u32;
            // After a missed tick anything in the unit may have a changed input: evaluate them all once.
            if stepped && !std::mem::replace(&mut self.queued[e as usize], true) {
                self.dirty.push(e);
            }
        }
        self.units[u] = Unit { frozen: false, ..unit };
    }

    /// Debugging: a unit that is simulated and at rest must show exactly the signals of its node.
    #[allow(dead_code)]
    fn units_check(&self) {
        for (u, unit) in self.units.iter().enumerate() {
            if unit.node == NONE || unit.touched || self.active.contains(&(u as u16)) {
                continue;
            }
            let bits = &unit.bits[unit.node as usize * unit.words..(unit.node as usize + 1) * unit.words];
            for (i, &e) in unit.comps.iter().chain(&unit.traces).chain(&unit.inputs).enumerate() {
                let member = i < unit.comps.len() + unit.traces.len();
                if (member && unit.frozen && self.direct.of(e as usize).is_empty() && self.writes.of(e as usize).is_empty()) || (member && unit.frozen) {
                    continue;
                }
                if self.on[e as usize] != (bits[i >> 6] >> (i & 63) & 1 == 1) {
                    panic!("units check: tick {}, unit {u} (frozen {}) node {}: entity {e} ({:?}, {}) is {} but the node says otherwise; its unit {}, feeds {:?}", self.tick, unit.frozen, unit.node, self.components.kinds[e as usize], if member { "member" } else { "input" }, self.on[e as usize], self.unit_of[e as usize], self.feeds.of(e as usize));
                }
            }
        }
    }

    /// At the end of a tick: every unit that had an input flip or is in a burst finds its next node.
    fn units_end_of_tick(&mut self) {
        // The common case first: a frozen unit in a burst, no input flipped, the next node is known.
        let mut i = 0;
        while i < self.active.len() {
            let u = self.active[i];
            let unit = &mut self.units[u as usize];
            if unit.frozen && !unit.touched && unit.next[unit.node as usize] != NONE {
                unit.skipped += unit.nodes[unit.node as usize].pending as u64;
                unit.node = unit.next[unit.node as usize];
                if unit.nodes[unit.node as usize].pending > 0 {
                    i += 1;
                } else {
                    self.active.swap_remove(i);
                }
                continue;
            }
            if !std::mem::replace(&mut unit.touched, true) {
                self.touched.push(u);
            }
            self.active.swap_remove(i);
        }
        if self.touched.is_empty() {
            return;
        }
        let mut list = std::mem::take(&mut self.touched);
        for &u16 in &list {
            let u = u16 as usize;
            let signature = std::mem::take(&mut self.units[u].signature);
            self.units[u].touched = false;
            let node = self.units[u].node;
            if self.units[u].frozen {
                let unit = &mut self.units[u];
                unit.skipped += unit.nodes[node as usize].pending as u64;
                let to = if signature == 0 { unit.next[node as usize] } else { unit.by_event.get(&(node, signature)).copied().unwrap_or(NONE) };
                if to != NONE {
                    unit.node = to;
                } else {
                    unit.misses += 1;
                    self.unit_thaw(u, true);
                    let (reached, _) = self.unit_intern(u);
                    self.unit_link(u, node, signature, reached);
                    self.units[u].node = reached;
                }
            } else if node == NONE {
                self.units[u].node = self.unit_intern(u).0;
            } else if signature != 0 || self.units[u].nodes[node as usize].pending > 0 {
                let (reached, known) = self.unit_intern(u);
                self.unit_link(u, node, signature, reached);
                self.units[u].node = reached;
                // Back at a node whose continuation is known: stop simulating the unit.
                if reached != NONE && known && (self.units[u].nodes[reached as usize].pending == 0 || self.units[u].next[reached as usize] != NONE) {
                    self.unit_freeze(u);
                }
            }
            let unit = &self.units[u];
            if unit.node != NONE && unit.nodes[unit.node as usize].pending > 0 {
                self.active.push(u16);
            } else if unit.node == NONE && !unit.frozen {
                // Not known where it stands (the table is full): look again at its next input change.
            }
        }
        list.clear();
        self.touched = list;
    }

    fn unit_link(&mut self, u: usize, from: u32, signature: u64, to: u32) {
        if from == NONE || to == NONE {
            return;
        }
        let unit = &mut self.units[u];
        if signature == 0 {
            if unit.next[from as usize] != NONE && unit.next[from as usize] != to {
                let old = unit.next[from as usize];
                let w = unit.words;
                let names: Vec<u32> = unit.comps.iter().chain(&unit.traces).chain(&unit.inputs).copied().collect();
                let differ: Vec<(u32, &str)> = (0..names.len()).filter(|i| (unit.bits[old as usize * w + (i >> 6)] ^ unit.bits[to as usize * w + (i >> 6)]) >> (i & 63) & 1 == 1).map(|i| (names[i], if i < unit.comps.len() { "comp" } else if i < unit.comps.len() + unit.traces.len() { "trace" } else { "input" })).collect();
                panic!("units: unit {u} at tick {} node {from} (pending {:?}) led to {old} (pending {:?}) and now {to} (pending {:?}); signals that differ: {differ:?}", self.tick, unit.pending(from), unit.pending(old), unit.pending(to));
            }
            unit.next[from as usize] = to;
        } else {
            let old = unit.by_event.insert((from, signature), to);
            if let Some(old) = old.filter(|old| *old != to) {
                let w = unit.words;
                let names: Vec<u32> = unit.comps.iter().chain(&unit.traces).chain(&unit.inputs).copied().collect();
                let differ: Vec<(u32, &str)> = (0..names.len()).filter(|i| (unit.bits[old as usize * w + (i >> 6)] ^ unit.bits[to as usize * w + (i >> 6)]) >> (i & 63) & 1 == 1).map(|i| (names[i], if i < unit.comps.len() { "comp" } else if i < unit.comps.len() + unit.traces.len() { "trace" } else { "input" })).collect();
                panic!("units: unit {u} at tick {} node {from} (pending {:?}) with inputs {signature:x} led to {old} (pending {:?}) and now {to} (pending {:?}); signals that differ: {differ:?}", self.tick, unit.pending(from), unit.pending(old), unit.pending(to));
            }
        }
    }
}
