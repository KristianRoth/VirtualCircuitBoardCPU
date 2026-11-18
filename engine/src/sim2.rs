//! The v2 engine: the fast one. It must show the same events as v1 (specs/vcb-engine-v2.md); every change
//! to it is an entry in specs/vcb-engine-v2-log.md. It started as a copy of v1's core (attempt 0).

use crate::board::Board;
use crate::format::Result;
use crate::ink::Ink;
use crate::netlist::{Components, Kind};
use crate::sim::{Options, Rng, Simulation};

pub struct Engine {
    pub components: Components,
    /// Per entity: a trace is on when any writer is on; a component's value is its output.
    on: Vec<bool>,
    /// Per trace: it went from off to on in the most recent Apply step.
    rose: Vec<bool>,
    rose_list: Vec<u32>,
    /// Components to evaluate on the next tick.
    dirty: Vec<u32>,
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
        let mut engine = Engine { on: vec![false; n], rose: vec![false; n], rose_list: Vec::new(), dirty: Vec::new(), queued: vec![false; n], clocks: Vec::new(), clock_interval, timers: Vec::new(), timer_ticks, rng, tick: 0, breakpoint_hit: false, vmem: board.vmem.clone(), address: 0, locked: false, load_pending: false, fetched: None, written: None, vmem_changed: false, pin_role: vec![0; n], components };
        for (role, pins) in [(1, &engine.components.address_pins), (2, &engine.components.content_pins)] {
            for pin in pins {
                engine.pin_role[*pin as usize] = role;
            }
        }
        let c = &engine.components;
        // Observed (gates board): at tick 0 every component already has the output it gets with all
        // inputs off, so Latch On and the inverting gates start on.
        for (e, kind) in c.kinds.iter().enumerate() {
            match kind {
                // Observed (general boards): a clock is on at tick 0 and flips at every multiple of the interval.
                Kind::Component(Ink::Clock) => {
                    engine.clocks.push(e as u32);
                    engine.on[e] = true;
                }
                // Observed (random-pulse board): unlike a clock, a timer starts off.
                Kind::Component(Ink::Timer) => engine.timers.push(e as u32),
                Kind::Component(Ink::LatchOn) => engine.on[e] = true,
                Kind::Component(ink @ (Ink::Not | Ink::Nor | Ink::Nand | Ink::Xnor)) => engine.on[e] = engine.evaluate(e, *ink),
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
                    if !engine.queued[reader as usize] {
                        engine.queued[reader as usize] = true;
                        engine.dirty.push(reader);
                    }
                }
            }
        }
        Ok(engine)
    }

    /// The output a component would have after this tick, from the trace states of the previous tick.
    fn evaluate(&self, e: usize, ink: Ink) -> bool {
        let reads = &self.components.reads[e];
        let active = reads.iter().filter(|t| self.on[**t as usize]).count();
        let risen = || reads.iter().filter(|t| self.rose[**t as usize]).count();
        match ink {
            Ink::Buffer | Ink::Or | Ink::Led | Ink::Breakpoint | Ink::Wireless(_) => active > 0,
            Ink::Not | Ink::Nor => active == 0,
            Ink::And => active > 0 && active == reads.len(),
            // Observed: a NAND with no inputs is off, so this is not the exact inverse of AND.
            Ink::Nand => active < reads.len(),
            Ink::Xor => active % 2 == 1,
            Ink::Xnor => active % 2 == 0,
            // Observed: toggles once per input that rose, so two inputs rising together cancel out.
            Ink::LatchOn | Ink::LatchOff => self.on[e] != (risen() % 2 == 1),
            // Clocks flip from the schedule in `step`, never from their inputs (observed).
            Ink::Clock => self.on[e],
            // Timers flip from the schedule too.
            Ink::Timer => self.on[e],
            other => unreachable!("{other} is not evaluated here"),
        }
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
        let c = &self.components;
        for &e in flipped {
            for &t in &c.writes[e as usize] {
                let now = c.writers[t as usize].iter().any(|w| self.on[*w as usize]);
                if now == self.on[t as usize] {
                    continue;
                }
                self.on[t as usize] = now;
                if now {
                    self.rose[t as usize] = true;
                    self.rose_list.push(t);
                }
                for &reader in &c.readers[t as usize] {
                    if !self.queued[reader as usize] {
                        self.queued[reader as usize] = true;
                        self.dirty.push(reader);
                    }
                }
            }
        }
    }
}

impl Simulation for Engine {
    fn step(&mut self) {
        self.tick += 1;
        let c = &self.components;

        let dirty = std::mem::take(&mut self.dirty);

        // 1. Evaluate: decide every flip before anything changes.
        let mut flips = Vec::new();
        for &e in &dirty {
            self.queued[e as usize] = false;
            let ink = match c.kinds[e as usize] {
                Kind::Component(ink) => ink,
                // A pin is a latch, except that VMem rejects its input while locked (the input is lost).
                Kind::Pin if self.locked => continue,
                Kind::Pin => Ink::LatchOff,
                Kind::Trace => unreachable!("traces are never queued"),
            };
            let next = if ink == Ink::Random {
                // Observed (random recorder boards): a Random draws every time it is evaluated, that is, whenever
                // one of its inputs changed. When an input rose the output is that draw. The draw made when the
                // input falls is used up without effect. The output stays on while an input stays on (observed).
                let draw = self.rng.next_bit();
                draw && c.reads[e as usize].iter().any(|t| self.on[*t as usize])
            } else {
                self.evaluate(e as usize, ink)
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

        // 2. Apply.
        for t in self.rose_list.drain(..) {
            self.rose[t as usize] = false;
        }
        self.breakpoint_hit = false;
        for &e in &flips {
            self.on[e as usize] = !self.on[e as usize];
            if c.kinds[e as usize] == Kind::Component(Ink::Breakpoint) && self.on[e as usize] {
                self.breakpoint_hit = true;
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
                let mut loaded = Vec::new();
                for (k, &pin) in self.components.content_pins.iter().enumerate() {
                    if self.on[pin as usize] != (word >> k & 1 == 1) {
                        self.on[pin as usize] = !self.on[pin as usize];
                        loaded.push(pin);
                    }
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
