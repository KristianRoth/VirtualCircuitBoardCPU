//! What share of a run's flips one-input gates make (Buffer, NOT, a one-input OR…: pure one-tick delays), to judge
//! whether collapsing them into delay lines could pay. Run from engine/:
//!   cargo run --release --example one_input -- [ticks] [board.vcb] [program.vcbasm or -]
use std::path::Path;
use vcb::netlist::{Components, Kind};
use vcb::sim::{Engine, Simulation};
use vcb::Ink;

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let arg = |i: usize, default: &str| args.get(i).cloned().unwrap_or(default.to_string());
    let count: u64 = arg(1, "3000000").parse().map_err(|e| format!("ticks: {e}"))?;
    let (board_path, program_path) = (arg(2, "../AoC2025/cpu-32bit-autorun-aoc1p1.vcb"), arg(3, "../AoC2025/days/day4/part2.vcbasm"));
    let mut board = vcb::Board::read(Path::new(&board_path))?;
    if program_path != "-" {
        let source = std::fs::read_to_string(&program_path).map_err(|e| e.to_string())?.replace("\r\n", "\n");
        vcb::asm::assemble(&source, &program_path, &mut board, false)?;
    }
    let c = Components::build(&board)?;
    let n = c.kinds.len();
    let options = vcb::sim::Options { timer_ticks: Some(1000) };
    let fast = vcb::sim2::Engine::new(&board, c.clone(), &options)?;
    // Left out: what v2 does not simulate, and what it flips from the clock schedule without evaluating.
    let scheduled = fast.scheduled();
    let live: Vec<bool> = (0..n).map(|e| fast.live()[e] && !scheduled[e]).collect();
    println!("flipped by the clock schedule, not counted below: {} components", scheduled.iter().filter(|s| **s).count());

    let mut engine = Engine::new(&board, c.clone(), &options)?;
    let mut previous = engine.state().to_vec();
    let mut flips = vec![0u64; n];
    let mut ticks = 0u64;
    while ticks < count && !engine.breakpoint_hit() {
        engine.step();
        ticks += 1;
        let state = engine.state();
        for e in 0..n {
            flips[e] += (state[e] != previous[e]) as u64;
        }
        previous.copy_from_slice(state);
    }

    let gate = |e: usize| matches!(c.kinds[e], Kind::Component(ink) if !matches!(ink, Ink::LatchOn | Ink::LatchOff | Ink::Clock | Ink::Timer | Ink::Random | Ink::Breakpoint));
    let one_input = |e: usize| gate(e) && c.reads[e].len() == 1;
    // A copy of one component: its only input trace has one writer. Private: no other component writes its traces.
    let copy = |e: usize| one_input(e) && c.writers[c.reads[e][0] as usize].len() == 1;
    let private = |e: usize| c.writes[e].iter().all(|t| c.writers[*t as usize].len() == 1);
    // In a chain: a copy whose source is itself a one-input gate, or that feeds only one-input gates.
    let chained = |e: usize| copy(e) && (one_input(c.writers[c.reads[e][0] as usize][0] as usize) || (private(e) && c.writes[e].iter().all(|t| c.readers[*t as usize].iter().all(|r| one_input(*r as usize)))));
    let components: Vec<usize> = (0..n).filter(|e| live[*e] && c.kinds[*e] != Kind::Trace).collect();
    let total: u64 = components.iter().map(|e| flips[*e]).sum();
    println!("{ticks} ticks; {} simulated components and pins made {total} flips ({:.2} per tick)", components.len(), total as f64 / ticks as f64);
    let rows: [(&str, &dyn Fn(usize) -> bool); 5] = [
        ("one-input gates", &one_input),
        ("  that never flipped", &|e| one_input(e) && flips[e] == 0),
        ("  copies of one component (input trace has one writer)", &copy),
        ("  copies that alone write their traces", &|e| copy(e) && private(e)),
        ("  in a chain of two or more", &chained),
    ];
    for (name, test) in rows {
        let set: Vec<usize> = components.iter().copied().filter(|e| test(*e)).collect();
        let f: u64 = set.iter().map(|e| flips[*e]).sum();
        println!("{name}: {} components, {f} flips = {:.2}% of all", set.len(), 100.0 * f as f64 / total as f64);
    }
    Ok(())
}
