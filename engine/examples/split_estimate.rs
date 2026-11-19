//! A follow-up to ternary_estimate (idea A1/A2 of specs/vcb-engine-v2.1-research2.md): what if a context is
//! split on some of the signals that vary at its start, so that each half has its own record ("1xxx is
//! always the same, 0xxx is always the same")?
//!
//! A context is the kind of event before a step and the address loaded there. It is split up to k times; each
//! split takes the start signal that leaves the fewest varying start signals per step in the two halves (ties:
//! the more even split). That is a stand-in for "the signal that makes most of the step unknown", chosen
//! from the whole run with hindsight and without any knowledge of the board. Everything else is counted as
//! in ternary_estimate, per k; k = 0 is ternary_estimate itself.
//!
//!   cargo run --release --example split_estimate -- <board.vcb> <program.vcbasm or -> [ticks] [sample]
use std::collections::{HashMap, HashSet};
use std::path::Path;
use vcb::netlist::{Components, Kind};
use vcb::sim::Simulation;
use vcb::Ink;

const DEPTHS: [usize; 6] = [0, 1, 2, 3, 4, 6];

fn mix(a: u64, b: u64) -> u64 {
    let x = (a ^ b).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    (x ^ x >> 29).wrapping_mul(0xbf58_476d_1ce4_e5b9).rotate_left(27) ^ b
}

/// The signals (as bits over the list of signals that matter) that are not the same in all of these states.
fn varying<'a>(states: impl Iterator<Item = &'a Vec<u64>>, words: usize) -> (Vec<u64>, usize) {
    let (mut all, mut any, mut count) = (vec![u64::MAX; words], vec![0u64; words], 0);
    for s in states {
        count += 1;
        for w in 0..words {
            all[w] &= s[w];
            any[w] |= s[w];
        }
    }
    ((0..words).map(|w| if count == 0 { 0 } else { any[w] & !all[w] }).collect(), count)
}

/// A sub-context: its steps, the first one's signals at its start and one tick before, and what differed later.
#[derive(Default)]
struct Sub {
    steps: u32,
    grew: u32,
    first: [Vec<u64>; 2],
    unknown: [Vec<u64>; 2],
}

/// The unknowns of one depth of splitting, followed through the run.
struct Follow {
    sub: Vec<u32>,
    subs: Vec<Sub>,
    unknown: Vec<bool>,
    earlier: Vec<bool>,
    next: Vec<bool>,
    ever: Vec<bool>,
    misses: usize,
    following: bool,
    /// Followed steps, flips, flips proven the same, flips of the variable part.
    total: [u64; 4],
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
    let mut sim = vec![false; n];
    let mut todo: Vec<usize> = (0..n).filter(|e| matches!(c.kinds[*e], Kind::Pin | Kind::Component(Ink::Breakpoint | Ink::Random))).collect();
    while let Some(e) = todo.pop() {
        if !std::mem::replace(&mut sim[e], true) {
            todo.extend((if c.kinds[e] == Kind::Trace { &c.writers[e] } else { &c.reads[e] }).iter().map(|x| *x as usize));
        }
    }
    let list: Vec<usize> = (0..n).filter(|e| sim[*e]).collect();
    let words = list.len().div_ceil(64);
    let pack = |state: &[bool]| {
        let mut bits = vec![0u64; words];
        list.iter().enumerate().filter(|(_, e)| state[**e]).for_each(|(i, _)| bits[i >> 6] |= 1 << (i & 63));
        bits
    };
    let (components, traces): (Vec<usize>, Vec<usize>) = list.iter().partition(|e| c.kinds[**e] != Kind::Trace);
    let fast = vcb::sim2::Engine::new(&board, c.clone(), &options)?;
    let scheduled = fast.scheduled();
    let counted: Vec<bool> = (0..n).map(|e| fast.live()[e] && !scheduled[e]).collect();
    let mut seed = 0x1234_5678_9abc_def1u64;
    let z: Vec<u64> = (0..n).map(|_| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    }).collect();

    // First run. A note is the start of a step: its context, its signals then and one tick before.
    // There is one more note than steps (the step the run ends in).
    let mut notes: Vec<((u8, u32), Vec<u64>, Vec<u64>)> = Vec::new();
    let mut known: Vec<bool> = Vec::new();
    {
        let mut engine = vcb::sim::Engine::new(&board, c.clone(), &options)?;
        let mut previous = engine.state().to_vec();
        let mut whole: u64 = list.iter().filter(|e| previous[**e]).fold(0, |h, e| h ^ z[*e]);
        let (mut start_whole, mut start_extra) = (whole, 0u64);
        let mut seen: HashSet<u64> = HashSet::new();
        notes.push(((0, u32::MAX), pack(&previous), pack(&previous)));
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
                known.push(!seen.insert(mix(mix(start_whole, start_extra), word)));
                start_whole = whole;
                start_extra = extra ^ (fetched.is_some() as u64 * 2 + wrote.is_some() as u64 + 7);
                let context = (fetched.is_some() as u8 * 2 + wrote.is_some() as u8, fetched.map_or(u32::MAX, |a| a as u32));
                notes.push((context, pack(state), pack(&previous)));
            }
            previous.copy_from_slice(state);
        }
    }

    // The splits: per depth, the sub-context of every note.
    let mut ids: HashMap<(u8, u32), u32> = HashMap::new();
    let mut level: Vec<u32> = notes.iter().map(|note| { let k = ids.len() as u32; *ids.entry(note.0).or_insert(k) }).collect();
    let mut count = ids.len();
    let mut levels = vec![level.clone()];
    for _ in 0..*DEPTHS.last().unwrap() {
        let mut groups: Vec<Vec<usize>> = vec![Vec::new(); count];
        level.iter().enumerate().for_each(|(i, id)| groups[*id as usize].push(i));
        count = 0;
        for group in &groups {
            let (differ, _) = varying(group.iter().map(|i| &notes[*i].1), words);
            // (varying start signals per step left in the halves, the smaller half) of the best signal so far.
            let mut best: Option<(usize, usize, usize)> = None;
            for bit in (0..words * 64).filter(|b| differ[b >> 6] >> (b & 63) & 1 == 1) {
                let on = |i: &&usize| notes[**i].1[bit >> 6] >> (bit & 63) & 1 == 1;
                let (v1, n1) = varying(group.iter().filter(on).map(|i| &notes[*i].1), words);
                let (v0, n0) = varying(group.iter().filter(|i| !on(i)).map(|i| &notes[*i].1), words);
                let left = |v: &Vec<u64>, k: usize| k * v.iter().map(|w| w.count_ones() as usize).sum::<usize>();
                let score = (left(&v0, n0) + left(&v1, n1), usize::MAX - n0.min(n1));
                if best.map_or(true, |b| score < (b.0, b.1)) {
                    best = Some((score.0, score.1, bit));
                }
            }
            for &i in group {
                level[i] = count as u32 + best.map_or(0, |(_, _, bit)| (notes[i].1[bit >> 6] >> (bit & 63) & 1) as u32);
            }
            count += 1 + best.is_some() as usize;
        }
        levels.push(level.clone());
    }

    let mut follows: Vec<Follow> = DEPTHS.iter().map(|depth| {
        let sub = levels[*depth].clone();
        let mut subs: Vec<Sub> = (0..=*sub.iter().max().unwrap()).map(|_| Sub::default()).collect();
        for (i, note) in notes.iter().enumerate() {
            let entry = &mut subs[sub[i] as usize];
            entry.steps += 1;
            for (k, state) in [&note.1, &note.2].into_iter().enumerate() {
                if entry.steps == 1 {
                    entry.first[k] = state.clone();
                    entry.unknown[k] = vec![0; words];
                } else {
                    let grew = (0..words).any(|w| (state[w] ^ entry.first[k][w]) & !entry.unknown[k][w] != 0);
                    (0..words).for_each(|w| entry.unknown[k][w] |= state[w] ^ entry.first[k][w]);
                    entry.grew += (grew && k == 0) as u32;
                }
            }
        }
        Follow { sub, subs, unknown: vec![false; n], earlier: vec![false; n], next: vec![false; n], ever: vec![false; n], misses: 0, following: false, total: [0; 4] }
    }).collect();

    // Second run: follow the unknowns through the sampled steps, for every depth at once.
    let unpack = |bits: &[u64], into: &mut [bool]| list.iter().enumerate().for_each(|(i, e)| into[*e] = bits[i >> 6] >> (i & 63) & 1 == 1);
    let begin = |f: &mut Follow, index: usize| {
        let entry = &f.subs[f.sub[index] as usize];
        unpack(&entry.unknown[0], &mut f.unknown);
        unpack(&entry.unknown[1], &mut f.earlier);
        f.ever.copy_from_slice(&f.unknown);
        f.following = index < known.len() && !known[index] && entry.steps > 1 && f.misses % sample == 0;
    };
    follows.iter_mut().for_each(|f| begin(f, 0));
    let mut engine = vcb::sim::Engine::new(&board, c.clone(), &options)?;
    let mut previous = engine.state().to_vec();
    let mut index = 0usize;
    let (mut flips, mut step_flips): (Vec<usize>, Vec<usize>) = (Vec::new(), Vec::new());
    let mut run = 0u64;
    while run < ticks && !engine.breakpoint_hit() && index < known.len() {
        engine.step();
        run += 1;
        let state = engine.state();
        if follows.iter().any(|f| f.following) {
            flips.clear();
            flips.extend(list.iter().copied().filter(|e| counted[*e] && state[*e] != previous[*e]));
            step_flips.extend(&flips);
        }
        for f in follows.iter_mut().filter(|f| f.following) {
            let (unknown, earlier) = (&f.unknown, &f.earlier);
            for &e in &components {
                let inputs = &c.reads[e];
                let known_input = |v: bool| inputs.iter().any(|t| !unknown[*t as usize] && previous[*t as usize] == v);
                let any_unknown = || inputs.iter().any(|t| unknown[*t as usize]);
                f.next[e] = match c.kinds[e] {
                    Kind::Component(Ink::Clock | Ink::Timer) => false,
                    Kind::Component(Ink::And | Ink::Nand) => !known_input(false) && any_unknown(),
                    Kind::Component(Ink::Buffer | Ink::Or | Ink::Nor | Ink::Not | Ink::Led | Ink::Breakpoint | Ink::Wireless(_)) => !known_input(true) && any_unknown(),
                    Kind::Pin | Kind::Component(Ink::LatchOn | Ink::LatchOff) => unknown[e] || inputs.iter().any(|t| unknown[*t as usize] || (previous[*t as usize] && earlier[*t as usize])),
                    _ => any_unknown(),
                };
            }
            for &t in &traces {
                let writers = &c.writers[t];
                f.next[t] = !writers.iter().any(|w| !f.next[*w as usize] && state[*w as usize]) && writers.iter().any(|w| f.next[*w as usize]);
            }
            for &e in &list {
                f.ever[e] |= f.next[e];
            }
            f.total[1] += flips.len() as u64;
            f.total[2] += flips.iter().filter(|e| !f.unknown[**e] && !f.next[**e]).count() as u64;
            std::mem::swap(&mut f.earlier, &mut f.unknown);
            std::mem::swap(&mut f.unknown, &mut f.next);
        }
        let (fetched, wrote) = engine.vmem_access();
        if fetched.is_some() || wrote.is_some() || engine.breakpoint_hit() {
            for f in follows.iter_mut() {
                if f.following {
                    f.total[0] += 1;
                    f.total[3] += step_flips.iter().filter(|e| f.ever[**e]).count() as u64;
                }
                f.misses += (!known[index] && f.subs[f.sub[index] as usize].steps > 1) as usize;
                begin(f, index + 1);
            }
            step_flips.clear();
            index += 1;
        }
        previous.copy_from_slice(state);
    }
    let missed = known.iter().filter(|k| !**k).count();
    println!("{run} ticks, {} steps, replay today misses {missed}", known.len());
    println!("splits  sub-contexts  followed  left to simulate  proven the same  new proof needed  missed steps alone in theirs");
    for (depth, f) in DEPTHS.iter().zip(&follows) {
        let pct = |a: u64, b: u64| 100.0 * a as f64 / b.max(1) as f64;
        let alone = (0..known.len()).filter(|i| !known[*i] && f.subs[f.sub[*i] as usize].steps == 1).count();
        let (all, grew) = f.subs.iter().fold((0u64, 0u64), |(a, g), s| (a + s.steps as u64, g + s.grew as u64));
        println!("{depth:>6}  {:>12}  {:>8}  {:>15.1}%  {:>14.1}%  {:>15.1}%  {:>27.1}%", f.subs.iter().filter(|s| s.steps > 0).count(), f.total[0], pct(f.total[3], f.total[1]), pct(f.total[2], f.total[1]), pct(grew, all), pct(alone as u64, missed as u64));
    }
    Ok(())
}
