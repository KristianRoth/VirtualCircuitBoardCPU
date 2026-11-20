//! Times v2 with and without the fixed/variable split (attempt 27) and prints the split's counters:
//!   cargo run --release --example split_time -- <board.vcb> <program.vcbasm or -> [ticks]
use std::path::Path;
use vcb::netlist::Components;
use vcb::sim::Simulation;

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let mut board = vcb::Board::read(Path::new(&args[1]))?;
    let source = if args[2] == "-" { board.assembly.clone() } else { std::fs::read_to_string(&args[2]).map_err(|e| e.to_string())?.replace("\r\n", "\n") };
    vcb::asm::assemble(&source, &args[2], &mut board, true)?;
    let ticks: u64 = args.get(3).map_or(20_000_000, |t| t.parse().unwrap());
    let options = vcb::sim::Options { timer_ticks: Some(1000) };
    let c = Components::build(&board)?;
    for on in [false, true] {
        let mut engine = vcb::sim2::Engine::new(&board, c.clone(), &options)?;
        engine.set_split(on);
        let start = std::time::Instant::now();
        while engine.tick() < ticks && !engine.breakpoint_hit() {
            engine.advance(ticks);
        }
        let (_, _, replayed) = engine.replay_stats();
        println!("split {on}: {:.3} s to tick {} ({:.1} M ticks/s), {:.0}% of ticks replayed; {:?}", start.elapsed().as_secs_f64(), engine.tick(), engine.tick() as f64 / start.elapsed().as_secs_f64() / 1e6, 100.0 * replayed as f64 / engine.tick() as f64, engine.split_counts());
        if cfg!(profiling) {
            println!("    simulated ticks {}, flips {}, evaluations {}, idle ticks {}", engine.profile.ticks_simulated, engine.profile.flips, engine.profile.evaluations, engine.profile.ticks_idle);
        }
    }
    Ok(())
}
