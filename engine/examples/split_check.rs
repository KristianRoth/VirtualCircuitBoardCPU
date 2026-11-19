//! Checks the fixed/variable split of v2 (attempt 27) against v2 without it, event by event:
//!   cargo run --release --example split_check -- <board.vcb> <program.vcbasm or -> [ticks] [stride]
use std::path::Path;
use vcb::netlist::Components;
use vcb::sim::Simulation;

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let mut board = vcb::Board::read(Path::new(&args[1]))?;
    let source = if args[2] == "-" { board.assembly.clone() } else { std::fs::read_to_string(&args[2]).map_err(|e| e.to_string())?.replace("\r\n", "\n") };
    vcb::asm::assemble(&source, &args[2], &mut board, true)?;
    let ticks: u64 = args.get(3).map_or(5_000_000, |t| t.parse().unwrap());
    // Stop and compare at least every `stride` ticks (and at every event).
    let stride: u64 = args.get(4).map_or(u64::MAX / 2, |t| t.parse().unwrap());
    let options = vcb::sim::Options { timer_ticks: Some(1000) };
    let c = Components::build(&board)?;
    let mut plain = vcb::sim2::Engine::new(&board, c.clone(), &options)?;
    plain.set_split(false);
    let mut split = vcb::sim2::Engine::new(&board, c.clone(), &options)?;
    let live = plain.live().to_vec();
    let mut events = 0u64;
    while plain.tick() < ticks && !plain.breakpoint_hit() {
        let until = (plain.tick() + stride).min(ticks);
        plain.advance(until);
        split.advance(until);
        events += 1;
        plain.sync();
        split.sync();
        let same = plain.tick() == split.tick() && plain.vmem_access() == split.vmem_access() && plain.breakpoint_hit() == split.breakpoint_hit();
        let differ = (0..live.len()).find(|e| live[*e] && plain.state()[*e] != split.state()[*e]);
        if !same || differ.is_some() || split.check_counts().is_some() {
            println!("event {events}: plain at tick {} {:?}, split at tick {} {:?}; first signal that differs: {differ:?} ({:?}); counts: {:?}", plain.tick(), plain.vmem_access(), split.tick(), split.vmem_access(), differ.map(|e| c.kinds[e]), split.check_counts());
            println!("{:?}", split.split_counts());
            for e in (0..live.len()).filter(|e| live[*e] && plain.state()[*e] != split.state()[*e]).take(12) { println!("{:?} plain {}: {}", c.kinds[e], plain.state()[e], split.split_debug(e as u32)); }
            return Err("the split differs".into());
        }
    }
    println!("{events} events to tick {}: the same. {:?}", plain.tick(), split.split_counts());
    Ok(())
}
