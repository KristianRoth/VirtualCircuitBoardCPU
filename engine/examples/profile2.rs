//! The profile of v2 for v2.1 (specs/vcb-engine-v2.1.md). Build it with the counters on, into its own folder:
//!   RUSTFLAGS="--cfg profiling -C target-cpu=native" cargo build --release --example profile2 --target-dir target/profiling
//!   target/profiling/release/examples/profile2 <board.vcb> <program.vcbasm or -> <ticks> [noreplay]
//! It prints what v2 did: how the ticks were crossed (replayed, jumped as idle, simulated), what the simulated
//! ticks held, and how often replay looked a state up or put one back. The sampling profiler gives the times.
use std::path::Path;
use vcb::netlist::Components;
use vcb::sim::Simulation;

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let mut board = vcb::Board::read(Path::new(&args[1]))?;
    let source = if args[2] == "-" { board.assembly.clone() } else { std::fs::read_to_string(&args[2]).map_err(|e| e.to_string())?.replace("\r\n", "\n") };
    vcb::asm::assemble(&source, &args[2], &mut board, true)?;
    let limit: u64 = args[3].parse().unwrap();
    let mut engine = vcb::sim2::Engine::new(&board, Components::build(&board)?, &vcb::sim::Options { timer_ticks: if board.inks.iter().any(|i| *i == vcb::Ink::Timer) { Some(1000) } else { None } })?;
    if args.get(4).is_some_and(|a| a == "noreplay") {
        engine.set_memory_limit(0);
    }
    let started = std::time::Instant::now();
    let mut stops = 0u64;
    while engine.tick() < limit && !engine.breakpoint_hit() {
        engine.advance(limit.min(engine.tick() + (1 << 20)));
        stops += 1;
    }
    let (t, s) = (engine.tick() as f64, started.elapsed().as_secs_f64());
    let p = engine.profile;
    let (nodes, bytes, replayed) = engine.replay_stats();
    let pct = |x: u64| 100.0 * x as f64 / t;
    println!("ticks {} in {s:.3} s = {:.1} M/s ({:.2} ns per tick overall)", engine.tick(), t / s / 1e6, s * 1e9 / t);
    println!("how the ticks were crossed: replayed {:.1}%, idle {:.1}%, simulated {:.1}%", pct(replayed), pct(p.ticks_idle), pct(p.ticks_simulated));
    println!("events (stops of advance): {stops}, one per {:.0} ticks; replayed steps {}; states looked up or stored {}; states restored {}; nodes kept {nodes} ({:.0} MiB)", t / stops as f64, p.replay_steps, p.interns, p.restores, bytes as f64 / (1 << 20) as f64);
    println!("in a simulated tick: {:.2} components evaluated, {:.2} flips; per simulated step (between two events): {:.1} ticks simulated", p.evaluations as f64 / p.ticks_simulated.max(1) as f64, p.flips as f64 / p.ticks_simulated.max(1) as f64, p.ticks_simulated as f64 / p.interns.max(1) as f64);
    Ok(())
}
