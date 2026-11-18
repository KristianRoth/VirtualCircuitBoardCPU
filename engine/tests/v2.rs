//! v2 must behave like v1 (specs/vcb-engine-v2.md). These tests compare more than the spec asks: every
//! signal at every tick, not only the events, so a difference is found at the tick where it starts.

use std::path::PathBuf;
use vcb::netlist::Components;
use vcb::sim::{Options, Simulation};
use vcb::{sim, sim2, Board};

fn boards() -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/boards");
    let mut boards: Vec<PathBuf> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).filter(|p| p.extension().is_some_and(|x| x == "vcb")).collect();
    boards.sort();
    boards
}

/// Steps both engines side by side and compares everything the trait shows after every tick.
fn same_for(board: &Board, ticks: u64, name: &str) {
    let options = Options { timer_ticks: Some(3) };
    let Ok(components) = Components::build(board) else { return };
    let mut v1 = sim::Engine::new(board, components.clone(), &options).unwrap();
    let mut v2 = sim2::Engine::new(board, components, &options).unwrap();
    for _ in 0..ticks {
        v1.step();
        v2.step();
        let same = v1.state() == v2.state() && v1.breakpoint_hit() == v2.breakpoint_hit() && v1.vmem_access() == v2.vmem_access() && v1.vmem_changed() == v2.vmem_changed();
        assert!(same, "{name}: v2 differs from v1 at tick {}", v1.tick());
    }
    assert!(v1.vmem() == v2.vmem(), "{name}: VMem differs after {ticks} ticks");
}

#[test]
fn v2_equals_v1_on_every_fixture_board() {
    let boards = boards();
    assert!(boards.len() > 20);
    for path in boards {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let mut board = Board::read(&path).unwrap();
        if !board.assembly.is_empty() {
            let text = board.assembly.clone();
            let _ = vcb::asm::assemble(&text, &name, &mut board, true);
        }
        // The CPU boards are large: fewer ticks in a debug build, still several instructions.
        same_for(&board, if name.starts_with("cpu-") { 20_000 } else { 3_000 }, &name);
    }
}

/// About 20 seconds in a release build: cargo test --release -- --ignored
#[test]
#[ignore]
fn v2_equals_v1_on_a_whole_program() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/boards/cpu-aoc1p1.vcb");
    let mut board = Board::read(&path).unwrap();
    let text = board.assembly.clone();
    vcb::asm::assemble(&text, "cpu", &mut board, true).unwrap();
    same_for(&board, 44_000_000, "cpu-aoc1p1");
}
