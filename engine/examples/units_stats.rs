//! Part-level skipping at work: cargo run --release --example units_stats -- <board.vcb> <program or -> <ticks> [largest] [smallest]
//! `largest` 0: no units, and no whole-board replay either (the plain simulation, to compare with).
use std::path::Path;
use vcb::netlist::Components;
use vcb::sim::Simulation;

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let mut board = vcb::Board::read(Path::new(&args[1]))?;
    let source = if args[2] == "-" { board.assembly.clone() } else { std::fs::read_to_string(&args[2]).map_err(|e| e.to_string())?.replace("\r\n", "\n") };
    vcb::asm::assemble(&source, &args[2], &mut board, true)?;
    let limit: u64 = args[3].parse().unwrap();
    let largest: usize = args.get(4).map_or(512, |a| a.parse().unwrap());
    let smallest: usize = args.get(5).map_or(32, |a| a.parse().unwrap());
    let mut engine = vcb::sim2::Engine::new(&board, Components::build(&board)?, &vcb::sim::Options { timer_ticks: Some(1000) })?;
    engine.enable_units(largest.max(1), if largest == 0 { usize::MAX } else { smallest });
    let started = std::time::Instant::now();
    while engine.tick() < limit && !engine.breakpoint_hit() {
        engine.advance(limit);
    }
    let (t, s) = (engine.tick(), started.elapsed().as_secs_f64());
    let (units, skipped, misses, recorded, nodes) = engine.unit_stats();
    println!("ticks {t} in {s:.2} s = {:.1} M/s; {units} units; flips skipped {:.3} per tick; lookups failed {:.4} per tick; nodes looked up while simulating {:.4} per tick; nodes kept {nodes}", t as f64 / s / 1e6, skipped as f64 / t as f64, misses as f64 / t as f64, recorded as f64 / t as f64);
    Ok(())
}
