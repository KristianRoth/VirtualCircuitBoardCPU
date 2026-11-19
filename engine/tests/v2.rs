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
    assert!(v1.state().iter().zip(v2.state()).zip(v2.live()).all(|((a, b), live)| a == b || !live), "{name}: v2 differs from v1 at tick 0");
    for _ in 0..ticks {
        v1.step();
        v2.step();
        // v2 does not simulate what cannot reach a VMem pin, a Breakpoint or a Random; everything else must be equal.
        let signals = v1.state().iter().zip(v2.state()).zip(v2.live()).all(|((a, b), live)| a == b || !live);
        let same = signals && v1.breakpoint_hit() == v2.breakpoint_hit() && v1.vmem_access() == v2.vmem_access() && v1.vmem_changed() == v2.vmem_changed();
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

/// v2 moved by the event primitive, with limits that fall inside idle stretches, against v1 stepped tick by tick:
/// v2 may only stop where v1 has an event or at the limit, v1 may have no event at a tick v2 passed over, and at
/// every stop everything must be equal.
#[test]
fn v2_advancing_by_events_equals_v1() {
    let options = Options { timer_ticks: Some(7) };
    for path in boards() {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let mut board = Board::read(&path).unwrap();
        if !board.assembly.is_empty() {
            let text = board.assembly.clone();
            let _ = vcb::asm::assemble(&text, &name, &mut board, true);
        }
        let Ok(components) = Components::build(&board) else { continue };
        for clock_interval in [board.settings.clock_interval, 36] {
            board.settings.clock_interval = clock_interval;
            let mut v1 = sim::Engine::new(&board, components.clone(), &options).unwrap();
            let mut v2 = sim2::Engine::new(&board, components.clone(), &options).unwrap();
            let end = if name.starts_with("cpu-") { 20_000 } else { 3_000 };
            let mut stops = 0;
            for limit in [1u64, 2, 5, 37, 400, 3].iter().cycle() {
                if v2.tick() >= end {
                    break;
                }
                let until = v2.tick() + limit;
                v2.advance(until);
                assert!(v2.tick() <= until, "{name}: v2 went past its limit");
                while v1.tick() < v2.tick() {
                    v1.step();
                    let event = v1.breakpoint_hit() || v1.vmem_access() != (None, None);
                    assert!(!event || v1.tick() == v2.tick(), "{name}: v2 passed over v1's event at tick {}", v1.tick());
                }
                let event = v1.breakpoint_hit() || v1.vmem_access() != (None, None);
                assert!(event || v2.tick() == until, "{name}: v2 stopped at tick {} without an event or its limit", v2.tick());
                let signals = v1.state().iter().zip(v2.state()).zip(v2.live()).all(|((a, b), live)| a == b || !live);
                let same = signals && v1.breakpoint_hit() == v2.breakpoint_hit() && v1.vmem_access() == v2.vmem_access() && v1.vmem_changed() == v2.vmem_changed();
                assert!(same, "{name} (clock interval {clock_interval}): v2 differs from v1 at tick {}", v1.tick());
                stops += 1;
            }
            assert!(stops > 10 && v1.vmem() == v2.vmem(), "{name}: VMem differs");
        }
    }
}
