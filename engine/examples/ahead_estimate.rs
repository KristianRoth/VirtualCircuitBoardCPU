//! Idea 11 of the open list (specs/vcb-engine-v2.1.md): could another thread simulate the NEXT step while the
//! engine is still simulating this one? It would have to guess the state the next step starts in. This counts
//! how good such a guess can be, on the reference engine.
//!
//! A step runs from one VMem event to the next; its context is the kind of event before it and the address
//! loaded there. When step k starts, the state at its end is guessed in two ways from the last step of the
//! same context:
//! - **last value**: every signal ends as it ended last time;
//! - **last change**: every signal that changed last time changes again, the others stay;
//! - **no change**: every signal ends as it starts;
//! - **chosen per signal**: for each signal, the first of those three rules that was right for it last time;
//! - the first and the fourth again with a **longer context** (this step's and the step's before it);
//! - **the limit of the three rules**: a signal counts as wrong only if all three rules get it wrong (no way
//!   of choosing among them can do better).
//! The guess is good enough if step k+1 never looks at a signal that was guessed wrong (its read set, the
//! sharper one of readset_estimate: only the inputs that decide a gate). Then simulating step k+1 from the guess
//! does exactly what the real step does. Counted over the steps replay misses today whose step before was
//! also missed (only then is there time to run ahead).
//!
//!   cargo run --release --example ahead_estimate -- <board.vcb> <program.vcbasm or -> [ticks]
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

/// What is kept of a context's last step: how it ended, what it changed, and per signal which rules were right.
#[derive(Default)]
struct History {
    end: Vec<u64>,
    change: Vec<u64>,
    right: [Vec<u64>; 3],
}

impl History {
    /// The guesses for a step that starts in `start`: last value, last change, no change, chosen per signal.
    fn guesses(&self, start: &[u64]) -> [Vec<u64>; 4] {
        let value = self.end.clone();
        let change: Vec<u64> = start.iter().zip(&self.change).map(|(s, c)| s ^ c).collect();
        let chosen = (0..start.len()).map(|w| {
            let (a, b, c) = (self.right[0][w], self.right[1][w], self.right[2][w]);
            (a | !(b | c)) & value[w] | !a & b & change[w] | !a & !b & c & start[w]
        }).collect();
        [value, change, start.to_vec(), chosen]
    }

    fn learn(&mut self, start: &[u64], end: &[u64]) {
        let guesses = if self.end.is_empty() { None } else { Some(self.guesses(start)) };
        for k in 0..3 {
            self.right[k] = match &guesses {
                Some(g) => g[k].iter().zip(end).map(|(g, e)| !(g ^ e)).collect(),
                None => vec![0; end.len()],
            };
        }
        self.change = start.iter().zip(end).map(|(s, e)| s ^ e).collect();
        self.end = end.to_vec();
    }
}

const WAYS: [&str; 7] = ["last value", "last change", "no change", "chosen per signal", "last value, longer context", "chosen per signal, longer context", "the limit of the three rules"];

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
    // Per context, and per pair of contexts (this step's and the one before it): the last step.
    let mut last: HashMap<(u8, u32), History> = HashMap::new();
    let mut last_two: HashMap<((u8, u32), (u8, u32)), History> = HashMap::new();
    let mut context_before = (0u8, u32::MAX);
    // For the step now running: which signals were guessed wrong at its start, by each way (None: no guess was possible).
    let mut wrong: Vec<Option<Vec<u64>>> = vec![None; WAYS.len()];
    // And whether the components that changed in the last tick of the step before were the same as last time.
    let (mut last_extra, mut extra_right): (HashMap<(u8, u32), u64>, bool) = (HashMap::new(), false);
    let mut before_known = true;
    // Pairs of missed steps; pairs with every guess to try; per way: wrong signals, good guesses; any way good; chosen per signal good and the last tick right.
    let (mut pairs, mut tried, mut any, mut with_tick) = (0u64, 0u64, 0u64, 0u64);
    let (mut wrong_sum, mut good_sum) = ([0u64; 7], [0u64; 7]);
    let mut steps: Vec<Step> = Vec::new();
    let (mut ticks_read, mut ticks_sharp) = (0u64, 0u64);
    let (missed, missed_flips, missed_written, missed_net, after_rest) = (0u64, 0u64, 0u64, 0u64, 0u64);
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
        for &e in &changed {
            if write_stamp[e] != number {
                write_stamp[e] = number;
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
            // Was this step, started from the guess, the same step? Only if it read nothing guessed wrong.
            if !known && !before_known {
                pairs += 1;
                if wrong.iter().all(|w| w.is_some()) {
                    tried += 1;
                    let good: Vec<bool> = wrong.iter().map(|w| !sharp_set.iter().any(|x| w.as_ref().unwrap()[slot[*x] >> 6] >> (slot[*x] & 63) & 1 != 0)).collect();
                    for k in 0..WAYS.len() {
                        wrong_sum[k] += wrong[k].as_ref().unwrap().iter().map(|w| w.count_ones() as u64).sum::<u64>();
                        good_sum[k] += good[k] as u64;
                    }
                    any += good[..6].iter().any(|g| *g) as u64;
                    with_tick += (good[3] && extra_right) as u64;
                }
            }
            before_known = known;
            // The guesses for the step that starts now: made when this step started, from this step's context.
            let differ = |guess: &Vec<u64>| -> Vec<u64> { guess.iter().zip(&end_bits).map(|(g, e)| g ^ e).collect() };
            let one = last.get(&context).filter(|h| !h.end.is_empty()).map(|h| h.guesses(&start_bits));
            let two = last_two.get(&(context, context_before)).filter(|h| !h.end.is_empty()).map(|h| h.guesses(&start_bits));
            wrong = vec![None; WAYS.len()];
            if let Some(g) = &one {
                for k in 0..4 {
                    wrong[k] = Some(differ(&g[k]));
                }
                wrong[6] = Some((0..words).map(|w| (g[0][w] ^ end_bits[w]) & (g[1][w] ^ end_bits[w]) & (g[2][w] ^ end_bits[w])).collect());
            }
            if let Some(g) = &two {
                (wrong[4], wrong[5]) = (Some(differ(&g[0])), Some(differ(&g[3])));
            }
            last.entry(context).or_default().learn(&start_bits, &end_bits);
            last_two.entry((context, context_before)).or_default().learn(&start_bits, &end_bits);
            let extra_now = changed.iter().fold(0u64, |h, e| h ^ mix(z[*e], 99));
            extra_right = last_extra.insert(context, extra_now) == Some(extra_now);
            context_before = context;
            let mask = masks.entry(context).or_insert_with(|| (vec![0; words], vec![0; words]));
            read_set.drain(..).for_each(|x| mask.0[slot[x] >> 6] |= 1 << (slot[x] & 63));
            sharp_set.drain(..).for_each(|x| mask.1[slot[x] >> 6] |= 1 << (slot[x] & 63));
            steps.push(Step { context, start: std::mem::replace(&mut start_bits, end_bits), extra: start_extra, word, length, known });
            number += 1;
            (read_hash, sharp_hash, rest_hit) = (0, 0, None);
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
    let _ = (hits, today, ticks_read, ticks_sharp, after_rest, missed_flips, missed_written, missed_net, missed, &size, &pct);
    println!("steps replay misses with the step before missed too: {pairs}; with every guess to try (the contexts were met before): {tried}");
    println!("{:<36} {:>14} {:>30}", "way of guessing", "signals wrong", "next step read none of them");
    for k in 0..WAYS.len() {
        println!("{:<36} {:>14.0} {:>29.1}%", WAYS[k], wrong_sum[k] as f64 / tried.max(1) as f64, 100.0 * good_sum[k] as f64 / tried.max(1) as f64);
    }
    println!("any of the first six good: {:.1}%; chosen per signal good and the last tick's changes as last time: {:.1}%", 100.0 * any as f64 / tried.max(1) as f64, 100.0 * with_tick as f64 / tried.max(1) as f64);
    Ok(())
}
