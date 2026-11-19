//! The part of a board that runs by itself: entities whose whole history depends only on clocks, never on
//! VMem pins or Random. It repeats with a fixed period.  cargo run --release --example autonomous -- <board.vcb>
use vcb::netlist::{Components, Kind};
use vcb::sim::Simulation;
use vcb::Ink;
fn main() -> Result<(), String> {
    let board = vcb::Board::read(std::path::Path::new(&std::env::args().nth(1).ok_or("board")?))?;
    let c = Components::build(&board)?;
    let n = c.kinds.len();
    // Tainted: can be influenced by a pin, a Random or a Timer. Walk forwards from those.
    let mut tainted = vec![false; n];
    let mut todo: Vec<usize> = (0..n).filter(|e| matches!(c.kinds[*e], Kind::Pin | Kind::Component(Ink::Random | Ink::Timer))).collect();
    while let Some(e) = todo.pop() {
        if !std::mem::replace(&mut tainted[e], true) {
            todo.extend((if c.kinds[e] == Kind::Trace { &c.readers[e] } else { &c.writes[e] }).iter().map(|x| *x as usize));
        }
    }
    // Driven: downstream of a clock.
    let mut driven = vec![false; n];
    let mut todo: Vec<usize> = (0..n).filter(|e| c.kinds[*e] == Kind::Component(Ink::Clock)).collect();
    while let Some(e) = todo.pop() {
        if !std::mem::replace(&mut driven[e], true) {
            todo.extend((if c.kinds[e] == Kind::Trace { &c.readers[e] } else { &c.writes[e] }).iter().map(|x| *x as usize));
        }
    }
    let live = vcb::sim2::Engine::new(&board, c.clone(), &Default::default())?.live().to_vec();
    let auto: Vec<usize> = (0..n).filter(|e| driven[*e] && !tainted[*e]).collect();
    println!("entities that run by themselves (clock-driven, never touched by memory): {} ({} components)", auto.len(), auto.iter().filter(|e| c.kinds[**e] != Kind::Trace).count());
    // Where they hand over to the rest: self-running traces read by a tainted component.
    let out: Vec<usize> = auto.iter().copied().filter(|e| c.kinds[*e] == Kind::Trace && c.readers[*e].iter().any(|r| tainted[*r as usize] && (live[*r as usize]))).collect();
    println!("their traces that the memory-dependent, simulated part reads: {}", out.len());
    // Period and flips, by running v1.
    let mut engine = vcb::sim::Engine::new(&board, c.clone(), &Default::default())?;
    let ticks = 200_000u64;
    let mut history: Vec<Vec<bool>> = Vec::new();
    let mut flips = vec![0u64; n];
    let mut previous = engine.state().to_vec();
    for _ in 0..ticks {
        engine.step();
        let s = engine.state();
        for &e in &auto {
            flips[e] += (s[e] != previous[e]) as u64;
        }
        history.push(auto.iter().map(|e| s[*e]).collect());
        previous.copy_from_slice(s);
    }
    let period = (1..10_000).find(|p| (1000..3000).all(|t| history[t] == history[t + p]));
    let total: u64 = auto.iter().map(|e| flips[*e]).sum();
    let out_flips: u64 = out.iter().map(|e| flips[*e]).sum();
    println!("period: {period:?} ticks; flips per tick inside: {:.2}; of those on the hand-over traces: {:.3}", total as f64 / ticks as f64, out_flips as f64 / ticks as f64);
    for e in &out {
        let i = c.entity_of.iter().position(|x| *x == *e as u32).unwrap();
        println!("  hand-over trace at {},{}: {:.4} flips per tick, {} readers", i % c.width, i / c.width, flips[*e] as f64 / ticks as f64, c.readers[*e].len());
    }
    Ok(())
}
