//! What replay did on a run: cargo run --release --example replay_stats -- <board.vcb> <program.vcbasm or -> <ticks>
use std::path::Path;
use vcb::netlist::Components;
use vcb::sim::Simulation;

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let mut board = vcb::Board::read(Path::new(&args[1]))?;
    let source = if args[2] == "-" { board.assembly.clone() } else { std::fs::read_to_string(&args[2]).map_err(|e| e.to_string())?.replace("\r\n", "\n") };
    vcb::asm::assemble(&source, &args[2], &mut board, true)?;
    let limit: u64 = args[3].parse().unwrap();
    let mut engine = vcb::sim2::Engine::new(&board, Components::build(&board)?, &vcb::sim::Options { timer_ticks: Some(1000) })?;
    let started = std::time::Instant::now();
    let mut events = 0u64;
    let mut line = Vec::new();
    let (mut window, mut window_start, mut window_replayed) = (1u64, std::time::Instant::now(), 0u64);
    while engine.tick() < limit && !engine.breakpoint_hit() {
        engine.advance(limit.min(engine.tick() + (1 << 20)));
        events += 1;
        if engine.tick() >= window * 100_000_000 {
            let replayed = engine.replay_stats().2;
            line.push(format!("{:.0}M/s {:.0}%", 100.0 / window_start.elapsed().as_secs_f64(), (replayed - window_replayed) as f64 / 1e6));
            (window, window_start, window_replayed) = (window + 1, std::time::Instant::now(), replayed);
        }
    }
    let (nodes, bytes, replayed) = engine.replay_stats();
    let (t, s) = (engine.tick(), started.elapsed().as_secs_f64());
    println!("ticks {t} in {s:.2} s = {:.1} M/s; stops {events}; nodes {nodes}, {:.0} MiB ({:.0} bytes per node); replay crossed {:.1}% of the ticks", t as f64 / s / 1e6, bytes as f64 / (1 << 20) as f64, bytes as f64 / nodes.max(1) as f64, 100.0 * replayed as f64 / t as f64);
    if !line.is_empty() {
        println!("per 100 million ticks (speed, share replayed): {}", line.join(", "));
    }
    Ok(())
}
