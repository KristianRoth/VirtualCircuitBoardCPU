//! The fixed part of a step is applied from a record, only the variable part is simulated (v2 attempt 27;
//! "A2" in specs/vcb-engine-v2.1.md).
//!
//! A **step** runs from one event (a VMem load, a VMem write, a Breakpoint) to the next. Its **context** is
//! what replay keeps beside the signals (the VMem address, the lock, the place in the clock's cycle). Steps of
//! one context mostly do the same thing to different data.
//!
//! **Learning.** Per context the engine keeps the signals of its first step's start and which signals have
//! differed from them at the start of a later step: those are **unknown**, the rest have so far always been
//! the same.
//!
//! **Proof.** When a step starts that agrees with the first on every signal not yet unknown, it is simulated as
//! usual while the unknowns are followed through it by the rules of three-valued simulation: an AND with a
//! known input that is off is known, whatever its other inputs are; an OR with a known input that is on
//! likewise; a trace with a known writer that is on likewise; any other gate with an unknown input is unknown;
//! a latch is unknown from the moment one of its inputs may or may not have risen. A signal that never became
//! unknown is **fixed**: in every step that starts with the same fixed signals it does exactly what it did
//! here, whatever the unknown signals are. The proof needs no knowledge of the board. Its result is a
//! **record**: the fixed signals with their values at the start, and their flips with the tick of each.
//!
//! **Use.** A later step of the context whose start agrees with the record on the fixed signals (one masked
//! compare) simulates only the **variable** signals: the ones that were unknown at some tick, and the VMem pins,
//! Breakpoints and clock-driven components, which the tick handles itself. The fixed gates are left alone:
//! - a flip of a variable signal still adjusts the counts of the fixed gates that read it, but wakes none;
//! - a flip of a fixed signal that a variable gate reads is handed to it from the record at its tick;
//! - when the step ends (at its event, at the record's end, or when someone looks at the state), the fixed
//!   signals are set to where the record has them at that tick, the counts between fixed signals are brought
//!   up to date, and the fixed gates that read something which flipped in that last tick are queued.
//!
//! Events come from the pins, which are always simulated, so a step may end earlier or later than the record's
//! did: the record is used up to the tick the step is at and no further.
//!
//! The proof runs on another thread (attempt 28): a worker with its own copy of the circuit is handed the start of
//! a step, and the record it sends back is used from the next step of the context that fits it. The engine never
//! waits for it; what it simulates is the same whenever a record arrives, only how fast.
//!
//! A context keeps a few records (an older one was proven when less was unknown: it fits fewer starts and fixes
//! more) and earns a proof only after several steps that fitted none.
//!
//! Not on a board with Random ink or a simulated Timer (no replay table there to take the packed signals from).

use super::{g, gm, Engine, Op, NONE};
use crate::replay::{Meta, Table};
use std::collections::HashMap;
use std::sync::{mpsc, Arc, Mutex};

pub(super) const NORMAL: u8 = 0;
/// Only the variable part is simulated; the fixed part comes from `Split::record`.
pub(super) const SPLIT: u8 = 1;
/// A step is simulated whole, and the unknowns are followed through it.
pub(super) const PROOF: u8 = 2;

/// What was proven about the steps of a context that start like one of them.
#[derive(Clone)]
struct Record {
    /// The signals at the start of the proven step and the components that flip in its first tick (bits in
    /// `live_list` order), and which of each the proof took as known: a step fits the record if it agrees on
    /// those. (More than the fixed signals: a pin is always simulated, but the proof may have used its value.)
    start: Vec<u64>,
    first: Vec<u64>,
    known: Vec<u64>,
    known_first: Vec<u64>,
    /// Which signals are fixed (same bits).
    fixed: Vec<u64>,
    /// Per entity (one bit each, by entity number): simulated in such a step.
    variable: Vec<u64>,
    /// Every flip of a fixed signal: (tick of the step, entity, its value afterwards), in order.
    flips: Vec<(u32, u32, bool)>,
    /// Those of them that a variable gate or trace reads.
    timed: Vec<(u32, u32, bool)>,
    /// The length of the proven step, and what the fixed part comes to over all of it: the signals that
    /// differ at the end, and the changes to the counts of fixed readers.
    length: u32,
    net: Vec<u32>,
    counts: Vec<(u32, u16)>,
}

#[derive(Default, Clone)]
struct Context {
    steps: u32,
    /// The first step's signals at its start and the components about to flip then; and what differed later.
    start: Vec<u64>,
    first: Vec<u64>,
    unknown: Vec<u64>,
    unknown_first: Vec<u64>,
    /// The records proven so far, the one used last in front. An older record was proven when less was
    /// unknown: it fits fewer starts and fixes more, and it stays true.
    records: Vec<Box<Record>>,
    /// Steps that fitted no record since the last proof.
    wait: u32,
    /// A proof has been asked of a worker and has not come back.
    asked: bool,
}

/// Records kept per context.
const RECORDS: usize = 6;
/// A context gets a proof only after this many of its steps fitted no record, so that one which is rarely met,
/// or whose starts keep differing, costs little. With a worker a proof costs this thread nothing but a copy of
/// the start, and 2 was the best of 1, 2, 4 and 8. Proving on the step itself costs about ten steps, and a
/// record saves under half a step each time it is used: 8 was the best of 1, 4, 8 and 16 there.
const WAIT: u32 = 2;
const WAIT_ALONE: u32 = 8;
/// shortcut: no more proofs once this many records are held (about 8 KB each on a CPU board), never freed;
/// a board with many more contexts than that would want the least used dropped.
const MOST_RECORDS: usize = 50_000;

/// Counters, for the tools.
#[derive(Default, Clone, Copy, Debug)]
pub struct Counts {
    /// Steps simulated with a record, proofs made, records pushed out by newer ones.
    pub split_steps: u64,
    pub proofs: u64,
    pub dropped: u64,
    /// Steps that started at an event and were simulated whole.
    pub whole_steps: u64,
    /// Ticks simulated and flips made in steps with a record.
    pub split_ticks: u64,
    pub split_flips: u64,
}

/// A step to prove on another thread: where it starts and what is unknown there.
struct Job {
    key: Meta,
    tick: u64,
    bits: Vec<u64>,
    /// The components that flip in the first tick, as a list and as bits.
    first: Vec<u32>,
    first_bits: Vec<u64>,
    unknown: Vec<u64>,
    unknown_first: Vec<u64>,
}

struct Done {
    key: Meta,
    record: Option<Box<Record>>,
}

/// The threads that prove (each with its own copy of the circuit): steps go in, records come out.
pub(super) struct Workers {
    jobs: mpsc::SyncSender<Job>,
    done: Mutex<mpsc::Receiver<Done>>,
}

/// A worker gives up on a step with no event after this many ticks.
const LONGEST: u64 = 100_000;

#[derive(Default, Clone)]
pub(super) struct Split {
    pub enabled: bool,
    /// How many threads prove (0: the engine proves itself, on the step it is simulating).
    pub threads: usize,
    workers: Option<Arc<Workers>>,
    /// This engine is a worker's copy: a finished record is left in `made`.
    worker: bool,
    made: Option<Box<Record>>,
    pub mode: u8,
    /// The next tick this mode needs to be stepped at (the idle jump goes no further).
    pub due: u64,
    /// The tick before this one had an event: a step starts here.
    pub at_event: bool,
    /// Records held in all contexts.
    records: usize,
    pub counts: Counts,
    contexts: HashMap<Meta, Context>,
    key: Meta,
    /// The tick the current step started at.
    start: u64,
    left: u64,
    record: Option<Box<Record>>,
    /// The next entry of the record's `timed`.
    cursor: usize,
    /// In the last tick: the variable traces that changed, and the pins VMem loaded.
    traces: Vec<(u32, bool)>,
    sources: Vec<(u32, bool)>,
    pub loaded: Vec<u32>,
    scratch: Vec<u64>,
    first_list: Vec<u32>,
    /// Work lists of the proof, kept so that a tick allocates nothing.
    lists: [Vec<u32>; 3],
    sums: Vec<u16>,
    // The proof.
    unknown: Vec<bool>,
    ever: Vec<bool>,
    /// The tick at which an entity's `unknown` last changed.
    changed: Vec<u64>,
    /// The signals as they were a tick ago.
    before: Vec<bool>,
    queue: Vec<u32>,
    queued: Vec<u64>,
    flips: Vec<(u32, u32, bool)>,
    start_bits: Vec<u64>,
    first_bits: Vec<u64>,
    known: (Vec<u64>, Vec<u64>),
}

#[inline(always)]
fn bit(bits: &[u64], e: u32) -> bool {
    g(bits, (e >> 6) as usize) >> (e & 63) & 1 != 0
}

impl Engine {
    /// For tools: the first live entity whose count is not the number of its inputs that are on (None: all right).
    /// Only meaningful after `sync`.
    pub fn check_counts(&self) -> Option<String> {
        self.live_list.iter().find_map(|&e| {
            let cell = &self.cell[e as usize];
            let inputs = if cell.op == Op::Trace { self.writers.of(e as usize) } else { self.reads.of(e as usize) };
            let on = inputs.iter().filter(|i| self.cell[**i as usize].on).count() as u16;
            (on != cell.count).then(|| format!("entity {e} ({}): count {} but {on} inputs are on", cell.op as u8, cell.count))
        })
    }

    /// For tools: what the record last used says about an entity.
    pub fn split_debug(&self, e: u32) -> String {
        let Some(record) = self.split.contexts.get(&self.split.key).and_then(|c| c.records.first()) else { return "no record".into() };
        let flips: Vec<_> = record.flips.iter().filter(|f| f.1 == e).collect();
        let inputs: Vec<_> = (if self.cell[e as usize].op == Op::Trace { self.writers.of(e as usize) } else { self.reads.of(e as usize) }).iter().map(|i| (*i, self.cell[*i as usize].op as u8, bit(&record.variable, *i), self.cell[*i as usize].on, record.flips.iter().filter(|f| f.1 == *i).count())).collect();
        format!("step from tick {} left at {}, record length {}; entity {e} variable: {}, on {}, count {}, its fixed flips {flips:?}; inputs (id, op, variable, on, fixed flips): {inputs:?}; timed {}", self.split.start, self.split.left, record.length, bit(&record.variable, e), self.cell[e as usize].on, self.cell[e as usize].count, record.timed.len())
    }

    pub fn split_counts(&self) -> Counts {
        self.split.counts
    }

    /// Switches the fixed/variable split on or off (it is on where it can be).
    /// How many threads prove (1 unless set). 0: the engine proves itself, on a step it is simulating, which
    /// makes a run's speed repeatable.
    pub fn set_split_threads(&mut self, threads: usize) {
        self.leave_mode();
        (self.split.threads, self.split.workers) = (threads, None);
    }

    pub fn set_split(&mut self, on: bool) {
        self.leave_mode();
        self.split.enabled = on && self.replay.is_some() && self.timers.is_empty();
        self.split.at_event = false;
    }

    /// The components that flip in the next tick, as bits in `live_list` order, into `scratch`.
    fn first_flips(&mut self) -> Vec<u64> {
        let mut bits = std::mem::take(&mut self.split.scratch);
        bits.clear();
        bits.resize(self.packed.len(), 0);
        let mut list = std::mem::take(&mut self.split.first_list);
        list.clear();
        let mut set = |e: u32| {
            let slot = self.slot[e as usize] as usize;
            bits[slot >> 6] |= 1 << (slot & 63);
            list.push(e);
        };
        if self.forced.is_empty() {
            for &e in &self.dirty {
                let op = self.cell[e as usize].op;
                if !(op == Op::Pin && self.locked) && self.evaluate(e as usize, op) != self.cell[e as usize].on {
                    set(e);
                }
            }
        } else {
            self.forced.iter().for_each(|e| set(*e));
        }
        self.split.first_list = list;
        bits
    }

    /// A step starts (the tick before had an event, and replay does not know what follows): use the context's
    /// record if it fits, or learn from this start, and prove if it is time.
    #[inline(never)]
    pub(super) fn begin_step(&mut self) {
        self.split.at_event = false;
        self.collect();
        let words = self.packed.len() - 1;
        let first = self.first_flips();
        let key = self.meta();
        let packed = &self.packed[..words];
        let context = self.split.contexts.entry(key).or_default();
        let fits = |record: &Record| (0..words).all(|w| (packed[w] ^ record.start[w]) & record.known[w] | (first[w] ^ record.first[w]) & record.known_first[w] == 0);
        let mut job: Option<Job> = None;
        'step: {
            if let Some(found) = context.records.iter().position(|record| fits(record)) {
                let record = context.records.remove(found);
                // What is queued is replaced by its outcome, and of that only the variable part is kept: the
                // fixed components that flip in the first tick are in the record.
                for e in self.dirty.drain(..) {
                    (self.cell[e as usize].queued, self.cell[e as usize].toggle) = (false, false);
                }
                self.forced.clear();
                self.forced.extend(self.split.first_list.iter().filter(|e| bit(&record.variable, **e)));
                self.split.due = self.tick + record.timed.first().map_or(record.length, |t| t.0.min(record.length)) as u64;
                (self.split.mode, self.split.key, self.split.start, self.split.cursor) = (SPLIT, key, self.tick, 0);
                self.split.record = Some(record);
                self.split.traces.clear();
                self.split.loaded.clear();
                self.split.counts.split_steps += 1;
                break 'step;
            }
            context.steps += 1;
            if context.steps == 1 {
                (context.start, context.first) = (packed.to_vec(), first[..words].to_vec());
                (context.unknown, context.unknown_first) = (vec![0; words], vec![0; words]);
            } else {
                let mut grew = false;
                for w in 0..words {
                    let (a, b) = (packed[w] ^ context.start[w], first[w] ^ context.first[w]);
                    grew |= a & !context.unknown[w] != 0 || b & !context.unknown_first[w] != 0;
                    context.unknown[w] |= a;
                    context.unknown_first[w] |= b;
                }
                context.wait += 1;
                // Not when something new differs in this very step, or before the context has earned a proof.
                let wait = if self.split.threads > 0 { WAIT } else { WAIT_ALONE };
                if !(grew || context.wait < wait || context.asked || self.split.records >= MOST_RECORDS) {
                    context.wait = 0;
                    context.asked = self.split.threads > 0;
                    job = Some(Job { key, tick: self.tick, bits: packed.to_vec(), first: self.split.first_list.clone(), first_bits: first[..words].to_vec(), unknown: context.unknown.clone(), unknown_first: context.unknown_first.clone() });
                }
            }
            self.split.counts.whole_steps += 1;
        }
        if self.split.threads > 0 {
            // A worker proves from a copy of this start; the step is simulated meanwhile.
            self.split.scratch = first;
            if let Some(job) = job {
                self.ask(job);
            }
        } else if let Some(job) = job {
            // Prove on this step.
            self.start_proof(job.key, job.unknown, job.unknown_first, &job.first_bits);
        } else {
            self.split.scratch = first;
        }
    }

    /// Sets up the proof of the step that starts here.
    fn start_proof(&mut self, key: Meta, unknown_bits: Vec<u64>, unknown_first: Vec<u64>, first: &[u64]) {
        let words = self.packed.len() - 1;
        let split = &mut self.split;
        let n = self.cell.len();
        if split.unknown.is_empty() {
            (split.unknown, split.ever, split.before) = (vec![false; n], vec![false; n], vec![false; n]);
            (split.changed, split.queued) = (vec![u64::MAX; n], vec![u64::MAX; n]);
        }
        for (i, &e) in self.live_list.iter().enumerate() {
            let e = e as usize;
            let unknown = unknown_bits[i >> 6] >> (i & 63) & 1 != 0;
            (split.unknown[e], split.ever[e], split.before[e], split.changed[e], split.queued[e]) = (unknown, unknown, self.cell[e].on, u64::MAX, u64::MAX);
        }
        split.start_bits.clear();
        split.start_bits.extend_from_slice(&self.packed[..words]);
        split.first_bits.clear();
        split.first_bits.extend_from_slice(first);
        // `scratch` keeps which components may or may not flip in the first tick, for that tick.
        split.known = (unknown_bits.iter().map(|w| !w).collect(), unknown_first.iter().map(|w| !w).collect());
        split.scratch = unknown_first;
        split.flips.clear();
        split.queue.clear();
        split.loaded.clear();
        (split.mode, split.key, split.start, split.due) = (PROOF, key, self.tick, self.tick + 1);
        split.counts.proofs += 1;
    }

    /// Hands a step to the workers (started at the first one). If they are too busy, it is not proven now.
    fn ask(&mut self, job: Job) {
        if self.split.workers.is_none() {
            let (jobs, taken) = mpsc::sync_channel::<Job>(256);
            let (finished, done) = mpsc::channel::<Done>();
            let taken = Arc::new(Mutex::new(taken));
            for _ in 0..self.split.threads {
                // A copy of the circuit without the replay table (one is needed, empty, for the packed signals).
                let (table, split) = (self.replay.take(), std::mem::take(&mut self.split));
                let mut engine = self.clone();
                (self.replay, self.split) = (table, split);
                engine.replay = Some(Table::new(self.packed.len() - 1, 0));
                (engine.split.worker, engine.split.due) = (true, u64::MAX);
                let (taken, finished) = (taken.clone(), finished.clone());
                std::thread::spawn(move || loop {
                    let Ok(job) = taken.lock().unwrap().recv() else { return };
                    let key = job.key;
                    let record = engine.prove(job);
                    if finished.send(Done { key, record }).is_err() {
                        return;
                    }
                });
            }
            self.split.workers = Some(Arc::new(Workers { jobs, done: Mutex::new(done) }));
        }
        let key = job.key;
        if self.split.workers.as_ref().unwrap().jobs.try_send(job).is_ok() {
            self.split.counts.proofs += 1;
        } else if let Some(context) = self.split.contexts.get_mut(&key) {
            context.asked = false;
        }
    }

    /// Takes in the records the workers have finished.
    #[inline(always)]
    fn collect(&mut self) {
        let Some(workers) = self.split.workers.take() else { return };
        if let Ok(done) = workers.done.try_lock() {
            while let Ok(Done { key, record }) = done.try_recv() {
                if let Some(context) = self.split.contexts.get_mut(&key) {
                    context.asked = false;
                }
                if let Some(record) = record {
                    self.install(key, record);
                }
            }
        }
        self.split.workers = Some(workers);
    }

    /// A new record for a context; the one used longest ago makes room.
    fn install(&mut self, key: Meta, record: Box<Record>) {
        if let Some(context) = self.split.contexts.get_mut(&key) {
            context.records.insert(0, record);
            self.split.records += 1;
            if context.records.len() > RECORDS {
                context.records.pop();
                self.split.records -= 1;
                self.split.counts.dropped += 1;
            }
        }
    }

    /// On a worker's copy of the circuit: puts it at the start of a step and proves the step.
    fn prove(&mut self, job: Job) -> Option<Box<Record>> {
        let words = self.packed.len() - 1;
        for e in self.dirty.drain(..) {
            (self.cell[e as usize].queued, self.cell[e as usize].toggle) = (false, false);
        }
        self.forced.clear();
        self.forced.extend_from_slice(&job.first);
        for (i, &e) in self.live_list.iter().enumerate() {
            self.cell[e as usize].on = job.bits[i >> 6] >> (i & 63) & 1 != 0;
        }
        for &e in &self.live_list {
            let inputs = if self.cell[e as usize].op == Op::Trace { self.writers.of(e as usize) } else { self.reads.of(e as usize) };
            self.cell[e as usize].count = inputs.iter().filter(|i| self.cell[**i as usize].on).count() as u16;
        }
        self.packed[..words].copy_from_slice(&job.bits);
        (self.address, self.locked, self.load_pending) = (job.key.address as usize, job.key.locked, job.key.load_pending);
        (self.tick, self.breakpoint_hit, self.fetched, self.written) = (job.tick, false, None, None);
        (self.real, self.node, self.arrays_node) = (true, NONE, NONE);
        self.set_due();
        self.start_proof(job.key, job.unknown, job.unknown_first, &job.first_bits);
        let limit = job.tick + LONGEST;
        while self.split.mode == PROOF && self.tick < limit {
            if self.dirty.is_empty() && self.forced.is_empty() && !self.load_pending && !self.locked {
                let target = limit.min(self.clock_due).min(self.timer_due).min(self.schedule_due).min(self.split.due);
                if target > self.tick + 1 {
                    self.tick = target - 1;
                }
            }
            self.step_real();
        }
        (self.split.mode, self.split.due) = (NORMAL, u64::MAX);
        self.split.made.take()
    }

    /// Leaves whatever mode the engine is in, so that the circuit is whole: before anyone looks at it.
    pub(super) fn leave_mode(&mut self) {
        match self.split.mode {
            SPLIT => self.leave_split(),
            PROOF => (self.split.mode, self.split.due) = (NORMAL, u64::MAX),
            _ => {}
        }
    }

    /// After a tick in a mode other than the normal one.
    #[inline(never)]
    pub(super) fn mode_tick(&mut self) {
        let event = self.breakpoint_hit || self.fetched.is_some() || self.written.is_some();
        if self.split.mode == SPLIT {
            self.split.counts.split_ticks += 1;
            self.split.counts.split_flips += self.flips.len() as u64;
            let record = self.split.record.as_ref().unwrap();
            let offset = (self.tick - self.split.start) as u32;
            if event || offset >= record.length {
                self.leave_split();
            } else {
                let next = record.timed.get(self.split.cursor).map_or(record.length, |t| t.0.min(record.length));
                self.split.due = self.split.start + next as u64;
            }
        } else {
            self.proof_tick(event);
        }
    }

    /// `propagate` while only the variable part is simulated. `inject`: this is the tick's own call (not VMem's
    /// for the pins it loaded), so the record's fixed flips of this tick are handed to their variable readers.
    pub(super) fn propagate_split(&mut self, flipped: &[u32], inject: bool) {
        let record = self.split.record.take().unwrap();
        let variable = &record.variable;
        let offset = (self.tick - self.split.start) as u32;
        let from = self.split.cursor;
        let mut to = from;
        if inject {
            self.split.traces.clear();
            self.split.loaded.clear();
            while to < record.timed.len() && record.timed[to].0 <= offset {
                debug_assert!(record.timed[to].0 == offset, "a tick with a fixed flip for the variable part was jumped over");
                to += 1;
            }
            self.split.cursor = to;
        }
        let timed = &record.timed[from..to];
        // First every writer count, then the traces (see `propagate`).
        for &e in flipped {
            let step = if g(&self.cell, e as usize).on { 1 } else { u16::MAX };
            for i in g(&self.writes.start, e as usize) as usize..g(&self.writes.start, e as usize + 1) as usize {
                let c = &mut gm(&mut self.cell, g(&self.writes.items, i) as usize).count;
                *c = c.wrapping_add(step);
            }
        }
        for &(_, e, now) in timed {
            if g(&self.cell, e as usize).op != Op::Trace {
                let step = if now { 1 } else { u16::MAX };
                for i in g(&self.writes.start, e as usize) as usize..g(&self.writes.start, e as usize + 1) as usize {
                    let t = g(&self.writes.items, i);
                    if bit(variable, t) {
                        let c = &mut gm(&mut self.cell, t as usize).count;
                        *c = c.wrapping_add(step);
                    }
                }
            }
        }
        for &e in flipped {
            let now = g(&self.cell, e as usize).on;
            for i in g(&self.direct.start, e as usize) as usize..g(&self.direct.start, e as usize + 1) as usize {
                self.feed_split(variable, g(&self.direct.items, i), now);
            }
            for i in g(&self.writes.start, e as usize) as usize..g(&self.writes.start, e as usize + 1) as usize {
                self.trace_split(variable, g(&self.writes.items, i));
            }
        }
        for &(_, e, now) in timed {
            let lists = if g(&self.cell, e as usize).op == Op::Trace { &self.readers } else { &self.direct };
            for i in g(&lists.start, e as usize) as usize..g(&lists.start, e as usize + 1) as usize {
                let lists = if g(&self.cell, e as usize).op == Op::Trace { &self.readers } else { &self.direct };
                let reader = g(&lists.items, i);
                if bit(variable, reader) {
                    self.feed(reader, now);
                }
            }
            if g(&self.cell, e as usize).op != Op::Trace {
                for i in g(&self.writes.start, e as usize) as usize..g(&self.writes.start, e as usize + 1) as usize {
                    self.trace_split(variable, g(&self.writes.items, i));
                }
            }
        }
        self.split.record = Some(record);
    }

    /// A reader of a variable signal: a variable one is told as usual, a fixed one only has its count adjusted.
    #[inline(always)]
    fn feed_split(&mut self, variable: &[u64], reader: u32, now: bool) {
        if bit(variable, reader) {
            self.feed(reader, now);
        } else {
            let c = &mut gm(&mut self.cell, reader as usize).count;
            *c = c.wrapping_add(if now { 1 } else { u16::MAX });
        }
    }

    /// A trace whose writer count changed: a variable trace follows it and tells its readers.
    #[inline(always)]
    fn trace_split(&mut self, variable: &[u64], t: u32) {
        if !bit(variable, t) {
            return;
        }
        let now = g(&self.cell, t as usize).count > 0;
        if now == g(&self.cell, t as usize).on {
            return;
        }
        gm(&mut self.cell, t as usize).on = now;
        self.flip_bit(t);
        self.split.traces.push((t, now));
        for k in g(&self.readers.start, t as usize) as usize..g(&self.readers.start, t as usize + 1) as usize {
            self.feed_split(variable, g(&self.readers.items, k), now);
        }
    }

    /// The fixed readers of a fixed signal that flipped: their counts.
    fn count_fixed(&mut self, variable: &[u64], e: u32, now: bool, mut add: impl FnMut(&mut Engine, u32, u16)) {
        let step = if now { 1 } else { u16::MAX };
        let trace = self.cell[e as usize].op == Op::Trace;
        for k in 0..2 {
            let lists = match (trace, k) {
                (true, 0) => &self.readers,
                (false, 0) => &self.writes,
                (false, 1) => &self.direct,
                _ => continue,
            };
            for i in lists.start[e as usize] as usize..lists.start[e as usize + 1] as usize {
                let lists = match (trace, k) {
                    (true, _) => &self.readers,
                    (false, 0) => &self.writes,
                    _ => &self.direct,
                };
                let reader = lists.items[i];
                if !bit(variable, reader) {
                    add(self, reader, step);
                }
            }
        }
    }

    /// Makes the circuit whole again at the tick it is at: the fixed signals go where the record has them,
    /// the counts among them follow, and the fixed gates that read something which flipped in this tick are queued.
    #[cold]
    #[inline(never)]
    fn leave_split(&mut self) {
        let record = self.split.record.take().unwrap();
        let offset = (self.tick - self.split.start) as u32;
        (self.split.mode, self.split.due) = (NORMAL, u64::MAX);
        self.split.left = self.tick;
        let done = record.flips.partition_point(|f| f.0 <= offset);
        if offset == record.length {
            for &e in &record.net {
                self.cell[e as usize].on ^= true;
                self.flip_bit(e);
            }
            for &(reader, change) in &record.counts {
                let c = &mut self.cell[reader as usize].count;
                *c = c.wrapping_add(change);
            }
        } else {
            for &(_, e, now) in &record.flips[..done] {
                self.cell[e as usize].on = now;
                self.flip_bit(e);
                self.count_fixed(&record.variable, e, now, |engine, reader, step| {
                    let c = &mut engine.cell[reader as usize].count;
                    *c = c.wrapping_add(step);
                });
            }
        }
        if offset == 0 {
            // No tick was simulated: the fixed components that flip in the first tick are still to come.
            for (i, &e) in self.live_list.iter().enumerate() {
                if (record.first[i >> 6] & record.fixed[i >> 6]) >> (i & 63) & 1 != 0 {
                    self.forced.push(e);
                }
            }
        } else {
            // What flipped in this last tick: the record's fixed signals, and the variable ones.
            let mut sources = std::mem::take(&mut self.split.sources);
            sources.clear();
            sources.extend(record.flips[..done].iter().rev().take_while(|f| f.0 == offset).map(|f| (f.1, f.2)));
            sources.extend(self.flips.iter().chain(&self.split.loaded).map(|e| (*e, self.cell[*e as usize].on)));
            sources.extend_from_slice(&self.split.traces);
            for &(e, now) in &sources {
                let lists = if self.cell[e as usize].op == Op::Trace { &self.readers } else { &self.direct };
                for i in lists.start[e as usize] as usize..lists.start[e as usize + 1] as usize {
                    let lists = if self.cell[e as usize].op == Op::Trace { &self.readers } else { &self.direct };
                    let reader = lists.items[i];
                    if bit(&record.variable, reader) {
                        continue;
                    }
                    let cell = &mut self.cell[reader as usize];
                    let wake = match cell.op {
                        Op::Latch | Op::Pin => {
                            cell.toggle ^= now;
                            now
                        }
                        Op::Scheduled | Op::Trace | Op::Dead => false,
                        // Waking a gate whose output does not change costs an evaluation and nothing else.
                        _ => true,
                    };
                    if wake && !cell.queued {
                        cell.queued = true;
                        self.dirty.push(reader);
                    }
                }
            }
            self.split.sources = sources;
        }
        if let Some(context) = self.split.contexts.get_mut(&self.split.key) {
            context.records.insert(0, record);
        }
    }

    /// Whether an entity is unknown at the tick before the one being computed (`unknown` holds the tick before
    /// this one until it is brought up to date).
    #[inline(always)]
    fn was_unknown(&self, e: u32) -> bool {
        self.split.unknown[e as usize] != (self.split.changed[e as usize] == self.tick - 1)
    }

    /// After a tick of a proof: follows the unknowns one tick on, notes the flips, and at the step's end makes
    /// the record.
    fn proof_tick(&mut self, event: bool) {
        let tick = self.tick;
        let offset = (tick - self.split.start) as u32;
        let [mut changed, mut flipped, mut traces] = std::mem::take(&mut self.split.lists);
        changed.clear(); // entities whose `unknown` changes in this tick
        flipped.clear();
        traces.clear();
        // The components whose inputs changed in the tick before: by the rules, from the tick before.
        let queue = std::mem::take(&mut self.split.queue);
        if offset == 1 {
            // The first tick's flips were decided before the step: unknown where they have differed.
            for (w, word) in self.split.scratch.iter().enumerate() {
                let mut word = *word;
                while word != 0 {
                    let e = self.live_list[w << 6 | word.trailing_zeros() as usize];
                    word &= word - 1;
                    if !self.split.unknown[e as usize] {
                        changed.push(e);
                    }
                }
            }
        } else {
            for &e in &queue {
                // In the second tick the queue holds every reader of something unknown. Only the known ones
                // must be looked at: leaving an unknown one unknown is on the safe side.
                if offset == 2 && self.split.unknown[e as usize] {
                    continue;
                }
                let inputs = self.reads.of(e as usize);
                let split = &self.split;
                let known = |v: bool| inputs.iter().any(|i| !split.unknown[*i as usize] && split.before[*i as usize] == v);
                let any = || inputs.iter().any(|i| split.unknown[*i as usize]);
                let unknown = match self.cell[e as usize].op {
                    Op::Or | Op::Breakpoint | Op::Nor => !known(true) && any(),
                    Op::And | Op::Nand => !known(false) && any(),
                    Op::Xor | Op::Xnor | Op::Random => any(),
                    // Unknown for good once an input may or may not have risen in the tick before.
                    Op::Latch | Op::Pin => split.unknown[e as usize] || inputs.iter().any(|i| split.unknown[*i as usize] || (split.before[*i as usize] && self.was_unknown(*i))),
                    Op::Scheduled | Op::Trace | Op::Dead => false,
                };
                if unknown != split.unknown[e as usize] {
                    changed.push(e);
                }
            }
        }
        // What changed its value in this tick: the components and pins that flipped, and the traces they write.
        for k in 0..self.flips.len() + self.split.loaded.len() {
            let e = if k < self.flips.len() { self.flips[k] } else { self.split.loaded[k - self.flips.len()] };
            for candidate in std::iter::once(e).chain(self.writes.of(e as usize).iter().copied()) {
                let on = self.cell[candidate as usize].on;
                if on != self.split.before[candidate as usize] {
                    self.split.before[candidate as usize] = on;
                    flipped.push(candidate);
                    self.split.flips.push((offset, candidate, on));
                }
            }
        }
        let split = &mut self.split;
        for &e in &changed {
            split.unknown[e as usize] ^= true;
            split.changed[e as usize] = tick;
            split.ever[e as usize] = true;
        }
        // The traces written by anything that changed (all of them in the first tick): a trace is on when a
        // writer is, so it is known if a known writer is on, or if all its writers are known.
        if offset == 1 {
            traces.extend(self.live_list.iter().filter(|e| self.cell[**e as usize].op == Op::Trace));
        } else {
            for &e in changed.iter().chain(&flipped) {
                traces.extend(self.writes.of(e as usize));
            }
        }
        for &t in &traces {
            let writers = self.writers.of(t as usize);
            let unknown = !writers.iter().any(|w| !split.unknown[*w as usize] && self.cell[*w as usize].on) && writers.iter().any(|w| split.unknown[*w as usize]);
            if unknown != split.unknown[t as usize] && split.changed[t as usize] != tick {
                split.unknown[t as usize] = unknown;
                split.changed[t as usize] = tick;
                split.ever[t as usize] |= unknown;
                changed.push(t);
            }
        }
        // Whoever reads something that changed (its value or whether it is known) is looked at in the next tick.
        let mut next = queue;
        next.clear();
        let mut wake = |e: u32, split: &mut Split| {
            let lists = if self.cell[e as usize].op == Op::Trace { &self.readers } else { &self.direct };
            for &reader in lists.of(e as usize) {
                if split.queued[reader as usize] != tick {
                    split.queued[reader as usize] = tick;
                    next.push(reader);
                }
            }
        };
        if offset == 1 {
            // Everything unknown now: what was at the start (or became so in this tick, which `changed` has).
            for w in 0..split.known.0.len() {
                let mut word = !split.known.0[w];
                while word != 0 {
                    let i = w << 6 | word.trailing_zeros() as usize;
                    word &= word - 1;
                    if i < self.live_list.len() && split.unknown[self.live_list[i] as usize] {
                        wake(self.live_list[i], split);
                    }
                }
            }
        }
        for &e in changed.iter().chain(&flipped) {
            wake(e, split);
        }
        split.due = if next.is_empty() && offset > 1 { u64::MAX } else { tick + 1 };
        split.queue = next;
        split.loaded.clear();
        split.lists = [changed, flipped, traces];
        if event {
            self.make_record(offset);
        }
    }

    /// The proof's step is over: what never became unknown is the fixed part.
    fn make_record(&mut self, length: u32) {
        (self.split.mode, self.split.due) = (NORMAL, u64::MAX);
        let n = self.cell.len();
        let words = self.packed.len() - 1;
        let mut variable = vec![0u64; n.div_ceil(64)];
        let mut set = |e: u32| variable[(e >> 6) as usize] |= 1 << (e & 63);
        for &e in &self.live_list {
            // The tick itself handles pins, Breakpoints and what the clock drives. A trace a pin writes changes
            // when VMem loads a word, which no proof covers. (Only simulated entities are ever asked about.)
            let op = self.cell[e as usize].op;
            if self.split.ever[e as usize] || matches!(op, Op::Pin | Op::Breakpoint | Op::Scheduled | Op::Random) {
                set(e);
            }
            if op == Op::Pin {
                self.writes.of(e as usize).iter().for_each(|t| set(*t));
            }
        }
        let mut fixed = vec![0u64; words];
        for (i, &e) in self.live_list.iter().enumerate() {
            fixed[i >> 6] |= (!bit(&variable, e) as u64) << (i & 63);
        }
        let flips: Vec<(u32, u32, bool)> = self.split.flips.iter().copied().filter(|f| !bit(&variable, f.1)).collect();
        let read_by_variable = |e: u32| {
            let lists: [&[u32]; 2] = if self.cell[e as usize].op == Op::Trace { [self.readers.of(e as usize), &[]] } else { [self.writes.of(e as usize), self.direct.of(e as usize)] };
            lists.iter().any(|list| list.iter().any(|r| bit(&variable, *r)))
        };
        let timed: Vec<(u32, u32, bool)> = flips.iter().copied().filter(|f| read_by_variable(f.1)).collect();
        // Over the whole step: which fixed signals end up different, and what that does to fixed readers' counts.
        // (`before` and `sums` serve as work arrays here: per entity, whether it ends up different, and its count's change.)
        let mut sums = std::mem::take(&mut self.split.sums);
        sums.resize(n, 0);
        let (mut net, mut touched): (Vec<u32>, Vec<u32>) = (Vec::new(), Vec::new());
        for &(_, e, _) in &flips {
            self.split.before[e as usize] = false;
        }
        for &(_, e, now) in &flips {
            self.split.before[e as usize] ^= true;
            self.count_fixed(&variable, e, now, |_, reader, step| {
                touched.push(reader);
                sums[reader as usize] = sums[reader as usize].wrapping_add(step);
            });
        }
        for &(_, e, _) in &flips {
            if std::mem::take(&mut self.split.before[e as usize]) {
                net.push(e);
            }
        }
        let mut counts: Vec<(u32, u16)> = Vec::new();
        for reader in touched {
            let sum = std::mem::take(&mut sums[reader as usize]);
            if sum != 0 {
                counts.push((reader, sum));
            }
        }
        self.split.sums = sums;
        let record = Record {
            start: std::mem::take(&mut self.split.start_bits),
            first: std::mem::take(&mut self.split.first_bits),
            known: std::mem::take(&mut self.split.known.0),
            known_first: std::mem::take(&mut self.split.known.1),
            fixed,
            variable,
            flips,
            timed,
            length,
            net,
            counts,
        };
        if self.split.worker {
            self.split.made = Some(Box::new(record));
        } else {
            self.install(self.split.key, Box::new(record));
        }
    }
}
