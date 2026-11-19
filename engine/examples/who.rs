//! Debug: what an entity is.  cargo run --release --example who -- <board.vcb> <entity> ...
use vcb::netlist::Components;
fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let board = vcb::Board::read(std::path::Path::new(&args[1]))?;
    let c = Components::build(&board)?;
    for a in &args[2..] {
        let e: usize = a.parse().unwrap();
        let i = c.entity_of.iter().position(|x| *x == e as u32).unwrap();
        println!("{e}: {:?} at {},{}; reads {:?} writes {:?} readers {:?} writers {:?}; address pin {} content pin {}", c.kinds[e], i % c.width, i / c.width, c.reads[e], c.writes[e], c.readers[e], c.writers[e], c.address_pins.contains(&(e as u32)), c.content_pins.contains(&(e as u32)));
    }
    Ok(())
}
