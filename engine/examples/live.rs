//! How much of a board v2 simulates: cargo run --release --example live -- <board.vcb>
use vcb::netlist::Components;
fn main() -> Result<(), String> {
    let board = vcb::Board::read(std::path::Path::new(&std::env::args().nth(1).ok_or("board")?))?;
    let c = Components::build(&board)?;
    let e = vcb::sim2::Engine::new(&board, c, &vcb::sim::Options { timer_ticks: Some(1000) })?;
    let live = e.live().iter().filter(|l| **l).count();
    println!("{live} of {} entities are simulated by v2 ({:.1}% dropped)", e.live().len(), 100.0 * (e.live().len() - live) as f64 / e.live().len() as f64);
    Ok(())
}
