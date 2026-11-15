//! Checks that the engine implements the model in specs/vcb-engine-v1.md.
//! These expectations are the spec's model, NOT observations of the game.
//! Observed fixtures live in tests/boards/ and are checked separately.

use std::path::Path;
use vcb::netlist::Components;
use vcb::sim::{Engine, Simulation, StopSet};
use vcb::textboard::{self, TextBoard};

fn load(name: &str) -> TextBoard {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("boards").join(name);
    textboard::parse(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// For each probe, the ticks (0..=ticks) at which its entity is on.
fn lit_ticks(tb: &TextBoard, ticks: u64) -> Vec<(String, Vec<u64>)> {
    let components = Components::build(&tb.board).unwrap();
    let entities: Vec<u32> = tb.probes.iter().map(|p| components.entity_at(p.x, p.y).unwrap()).collect();
    let mut lit: Vec<(String, Vec<u64>)> = tb.probes.iter().map(|p| (p.label.clone(), Vec::new())).collect();
    let mut engine = Engine::new(&tb.board, components).unwrap();
    engine.run_until(&StopSet { max_ticks: ticks, breakpoint: false }, |e| {
        for (i, entity) in entities.iter().enumerate() {
            if e.state()[*entity as usize] {
                lit[i].1.push(e.tick());
            }
        }
    });
    lit
}

#[test]
fn gates_board_follows_the_model() {
    // The tick at which each LED first lights; `None` = never. Once lit, every LED on this board stays lit.
    let expected: [(&str, Option<u64>); 24] = [
        ("latch_on", Some(1)),      // the Latch On trace is on at tick 0, the LED follows one tick later
        ("buffer", Some(2)),        // one more tick per gate
        ("not_no_input", Some(2)),  // NOT turns on at tick 1
        ("two_colours", Some(1)),   // trace colours merge
        ("no_write", None),         // a latch touching a trace directly drives nothing
        ("buffer_10", Some(2)),
        ("and_10", None),
        ("or_10", Some(2)),
        ("xor_10", Some(2)),
        ("not_10", None),
        ("nand_10", Some(2)),
        ("nor_10", None),
        ("xnor_10", None),
        ("and_11", Some(2)),
        ("xor_11", None),
        ("xnor_11", Some(2)),
        ("and_no_input", None),
        ("nand_no_input", Some(2)),
        ("xor_two_reads", Some(2)),       // two Read pixels on one trace count as one input
        ("latch_from_not", Some(3)),      // the NOT's trace rises at tick 1, the latch toggles at tick 2
        ("latch_two_rises", Some(3)),     // two inputs rising together toggle once
        ("latch_from_latch_on", None),    // a trace that is on from tick 0 has no rising edge
        ("shared_read_left", Some(1)),    // one Read pixel feeds both LEDs it touches
        ("shared_read_right", Some(1)),
    ];
    let lit = lit_ticks(&load("gates.txt"), 6);
    assert_eq!(lit.len(), expected.len());
    for ((label, ticks), (name, first)) in lit.iter().zip(expected) {
        assert_eq!(label, name);
        let want: Vec<u64> = first.map(|t| (t..=6).collect()).unwrap_or_default();
        assert_eq!(*ticks, want, "{label}");
    }
}

#[test]
fn clock_flips_every_interval_and_a_breakpoint_stops_the_run() {
    // Clock -> Write -> trace -> Read -> Breakpoint, interval 3.
    let tb = textboard::parse("clock_interval = 3\nprobe.clock = 0,0\nprobe.bp = 4,0\n\nCW5R!").unwrap();
    let lit = lit_ticks(&tb, 9);
    assert_eq!(lit[0].1, [3, 4, 5, 9]);
    assert_eq!(lit[1].1, [4, 5, 6]);

    let mut engine = Engine::new(&tb.board, Components::build(&tb.board).unwrap()).unwrap();
    let report = engine.run_until(&StopSet { max_ticks: 100, breakpoint: true }, |_| {});
    assert_eq!((report.tick, report.reasons), (4, vec!["breakpoint"]));

    let mut engine = Engine::new(&tb.board, Components::build(&tb.board).unwrap()).unwrap();
    let report = engine.run_until(&StopSet { max_ticks: 4, breakpoint: true }, |_| {});
    assert_eq!((report.tick, report.reasons), (4, vec!["breakpoint", "max-ticks"]));
}

#[test]
fn two_runs_give_the_same_state_every_tick() {
    let tb = load("gates.txt");
    let run = || {
        let mut states = Vec::new();
        let mut engine = Engine::new(&tb.board, Components::build(&tb.board).unwrap()).unwrap();
        engine.run_until(&StopSet { max_ticks: 20, breakpoint: false }, |e| states.push(e.state().to_vec()));
        states
    };
    assert!(run() == run());
}

#[test]
fn entities_are_numbered_by_their_first_pixel() {
    let tb = textboard::parse("\nB.5\n.BW").unwrap();
    let c = Components::build(&tb.board).unwrap();
    let at = |x: usize, y: usize| c.entity_at(16 + x, 16 + y);
    assert_eq!((at(0, 0), at(2, 0), at(1, 1), at(2, 1), at(1, 0)), (Some(0), Some(1), Some(2), Some(1), None));
    assert_eq!(c.writes[2], [1]); // the second buffer writes the trace through the Write pixel
    assert!(c.writes[0].is_empty());
}
