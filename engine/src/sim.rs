//! The v1 engine: dirty list + recompute. Design: specs/vcb-engine-v1.md, "Simulation core".

use crate::board::Board;
use crate::format::Result;
use crate::ink::Ink;
use crate::netlist::{Components, Kind};

/// What ends a run.
pub struct StopSet {
    pub max_ticks: u64,
    pub breakpoint: bool,
}

pub struct StopReport {
    pub tick: u64,
    /// Every condition that held on the final tick, in the framework spec's order.
    pub reasons: Vec<&'static str>,
}

pub trait Simulation {
    /// Advances one tick.
    fn step(&mut self);
    fn tick(&self) -> u64;
    /// On/off of every entity, in canonical entity order.
    fn state(&self) -> &[bool];
    /// True if a Breakpoint turned on during the last tick.
    fn breakpoint_hit(&self) -> bool;

    /// Runs until a stop condition holds. `each_tick` is called for the starting state and after every tick.
    fn run_until(&mut self, stops: &StopSet, mut each_tick: impl FnMut(&Self)) -> StopReport
    where
        Self: Sized,
    {
        loop {
            each_tick(self);
            let mut reasons = Vec::new();
            if stops.breakpoint && self.breakpoint_hit() {
                reasons.push("breakpoint");
            }
            if self.tick() >= stops.max_ticks {
                reasons.push("max-ticks");
            }
            if !reasons.is_empty() {
                return StopReport { tick: self.tick(), reasons };
            }
            self.step();
        }
    }
}

/// Run settings that do not come from the board file.
#[derive(Default)]
pub struct Options {
    /// How many ticks one Timer interval lasts. Required when the board has Timer ink,
    /// because the game's timers follow the wall clock.
    pub timer_ticks: Option<u64>,
}

/// MT19937, the generator the game uses for Random ink (std::mt19937 seeded with the board's seed).
/// Verified against 2 x 4064 recorded bits (random-vmem-seed-1 and -2).
struct Rng {
    state: [u32; 624],
    index: usize,
}

impl Rng {
    fn new(seed: u32) -> Rng {
        let mut state = [0u32; 624];
        state[0] = seed;
        for i in 1..624 {
            state[i] = 1812433253u32.wrapping_mul(state[i - 1] ^ (state[i - 1] >> 30)).wrapping_add(i as u32);
        }
        Rng { state, index: 624 }
    }

    fn next_u32(&mut self) -> u32 {
        if self.index == 624 {
            for k in 0..624 {
                let y = (self.state[k] & 0x8000_0000) | (self.state[(k + 1) % 624] & 0x7fff_ffff);
                self.state[k] = self.state[(k + 397) % 624] ^ (y >> 1) ^ if y & 1 == 1 { 0x9908_b0df } else { 0 };
            }
            self.index = 0;
        }
        let mut y = self.state[self.index];
        self.index += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^ (y >> 18)
    }

    /// The game uses the low bit of each draw.
    fn next_bit(&mut self) -> bool {
        self.next_u32() & 1 == 1
    }
}

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
        let mut engine = Engine { on: vec![false; n], rose: vec![false; n], rose_list: Vec::new(), dirty: Vec::new(), queued: vec![false; n], clocks: Vec::new(), clock_interval, timers: Vec::new(), timer_ticks, rng, tick: 0, breakpoint_hit: false, components };
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
                // Model: a timer starts like a clock.
                Kind::Component(Ink::Timer) => {
                    engine.timers.push(e as u32);
                    engine.on[e] = true;
                }
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

impl Simulation for Engine {
    fn step(&mut self) {
        self.tick += 1;
        let c = &self.components;

        let dirty = std::mem::take(&mut self.dirty);

        // 1. Evaluate: decide every flip before anything changes.
        let mut flips = Vec::new();
        for &e in &dirty {
            self.queued[e as usize] = false;
            let Kind::Component(ink) = c.kinds[e as usize] else { unreachable!("only components are queued") };
            let next = if ink == Ink::Random {
                // Observed (random recorder boards): a Random draws every time it is evaluated, that is, whenever
                // one of its inputs changed. When an input rose the output is that draw. The draw made when the
                // input falls is used up without effect. Model (question 14): the output is on only while an input is on.
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
        for &e in &flips {
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

    fn tick(&self) -> u64 {
        self.tick
    }

    fn state(&self) -> &[bool] {
        &self.on
    }

    fn breakpoint_hit(&self) -> bool {
        self.breakpoint_hit
    }
}
