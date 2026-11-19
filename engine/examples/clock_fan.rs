//! How far a board's clock reaches: cargo run --release --example clock_fan -- <board.vcb>
use vcb::netlist::{Components, Kind};
use vcb::Ink;
fn main() -> Result<(), String> {
    let board = vcb::Board::read(std::path::Path::new(&std::env::args().nth(1).ok_or("board")?))?;
    let c = Components::build(&board)?;
    let engine = vcb::sim2::Engine::new(&board, c.clone(), &vcb::sim::Options { timer_ticks: Some(1000) })?;
    println!("clock interval {}", board.settings.clock_interval);
    for e in 0..c.kinds.len() {
        if c.kinds[e] == Kind::Component(Ink::Clock) {
            let readers: Vec<u32> = c.writes[e].iter().flat_map(|t| c.readers[*t as usize].iter().copied()).collect();
            let mut inks: std::collections::BTreeMap<String, usize> = Default::default();
            for r in &readers {
                if let Kind::Component(ink) = c.kinds[*r as usize] {
                    *inks.entry(format!("{ink}")).or_default() += 1;
                }
            }
            println!("clock #{e}: simulated {}, writes {} traces, read by {} components: {inks:?}", engine.live()[e], c.writes[e].len(), readers.len());
        }
    }
    Ok(())
}
