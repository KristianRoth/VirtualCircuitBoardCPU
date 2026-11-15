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
    // The ticks at which each LED is lit.
    let always = |from: u64| (from..=6).collect::<Vec<u64>>();
    let expected: [(&str, Vec<u64>); 24] = [
        ("latch_on", always(1)),          // the Latch On trace is on at tick 0, the LED follows one tick later
        ("buffer", always(2)),            // one more tick per gate
        ("not_no_input", always(1)),      // inverting gates are already on at tick 0
        ("two_colours", always(1)),       // trace colours merge
        ("no_write", vec![]),             // a latch touching a trace directly drives nothing
        ("buffer_10", always(2)),
        ("and_10", vec![]),
        ("or_10", always(2)),
        ("xor_10", always(2)),
        ("not_10", vec![1]),              // on at tick 0, off once it sees its input
        ("nand_10", always(1)),
        ("nor_10", vec![1]),
        ("xnor_10", vec![1]),
        ("and_11", always(2)),
        ("xor_11", vec![]),
        ("xnor_11", always(1)),
        ("and_no_input", vec![]),
        ("nand_no_input", vec![]),        // off, unlike a NOT with no input
        ("xor_two_reads", always(2)),     // two Read pixels on one trace count as one input
        ("latch_from_not", always(2)),    // a trace that is on at tick 0 counts as a rising edge at tick 1
        ("latch_two_rises", vec![]),      // two inputs rising together toggle twice: no change
        ("latch_from_latch_on", always(2)),
        ("shared_read_left", always(1)),  // one Read pixel feeds both LEDs it touches
        ("shared_read_right", always(1)),
    ];
    let lit = lit_ticks(&load("gates.txt"), 6);
    assert_eq!(lit.len(), expected.len());
    for ((label, ticks), (name, want)) in lit.iter().zip(expected) {
        assert_eq!(label, name);
        assert_eq!(*ticks, want, "{label}");
    }
}

/// Asserts each probe of a board is on at exactly the given ticks (model expectations).
fn assert_lit(board: &str, ticks: u64, expected: &[(&str, &[u64])]) {
    let lit = lit_ticks(&load(board), ticks);
    assert_eq!(lit.len(), expected.len(), "{board}: number of probes");
    for ((label, got), (name, want)) in lit.iter().zip(expected) {
        assert_eq!(label, name, "{board}");
        assert_eq!(got, want, "{board}: {label}");
    }
}

#[test]
fn routing_a_board_follows_the_model() {
    assert_lit("routing-a.txt", 4, &[
        ("cross_vertical_off", &[]),             // the crossing traces stay separate
        ("cross_horizontal_on", &[1, 2, 3, 4]),
        ("cross_two_colours", &[1, 2, 3, 4]),    // a cross joins traces of different colours
        ("cross_write_read_on", &[1, 2, 3, 4]),  // Write and Read pixels are trace pixels at a cross
        ("cross_other_off", &[]),
        ("or_through_cross", &[2, 3, 4]),        // both halves of a gate split by a cross are one gate
        ("xor_through_cross", &[2, 3, 4]),
        ("xnor_through_cross", &[1]),            // on at tick 0, off once the far half sees its input
        ("and_through_cross", &[2, 3, 4]),
        ("tunnel_red_on", &[1, 2, 3, 4]),        // skips the tunnel that has another colour behind it
        ("tunnel_yellow_off", &[]),
        ("read_write_inside_trace", &[1, 2, 3, 4]),
        ("led_drives_led", &[2, 3, 4]),
    ]);
}

#[test]
fn routing_b_board_follows_the_model() {
    assert_lit("routing-b.txt", 4, &[
        ("bus_nine_off", &[]),                   // a bus matches traces by colour, not by position
        ("bus_two_on", &[1, 2, 3, 4]),
        ("bus_touching_colour_differs", &[]),    // the colour that counts is the pixel touching the bus
        ("bus_touching_colour_same", &[1, 2, 3, 4]),
        ("bus_through_cross", &[1, 2, 3, 4]),
        ("bus_through_tunnel", &[1, 2, 3, 4]),
        ("and_through_bus", &[2, 3, 4]),         // gates of the same ink touching one bus are one gate
        ("xor_through_bus", &[2, 3, 4]),
        ("not_through_bus", &[1]),
        ("mesh_same_colour", &[1, 2, 3, 4]),
        ("mesh_other_colour", &[]),
        ("wireless_0_on", &[2, 3, 4]),
        ("wireless_1_off", &[]),
    ]);
}

#[test]
fn general_boards_follow_the_model() {
    assert_lit("general.txt", 10, &[
        ("clock", &[4, 5, 6, 10]),               // interval 3: the clock flips at ticks 3, 6, 9
        ("clock_with_input", &[4, 5, 6, 10]),    // an input changes nothing
        ("latch_second_rise", &[2, 3]),          // a second input rising later toggles it back
        ("latch_fall_and_rise", &[2]),           // a falling input does nothing; the rising one toggles
        ("shared_write_a", &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]),
        ("shared_write_b", &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]),
        ("breakpoint_chain", &[7, 8, 9, 10]),
    ]);
    assert_lit("general-1.txt", 5, &[("clock_interval_1", &[2, 4])]);

    // The Breakpoint shares the LED's input, so the run stops on the tick the LED lights.
    let tb = load("general.txt");
    let mut engine = Engine::new(&tb.board, Components::build(&tb.board).unwrap()).unwrap();
    let report = engine.run_until(&StopSet { max_ticks: 100, breakpoint: true }, |_| {});
    assert_eq!((report.tick, report.reasons), (7, vec!["breakpoint"]));
}

#[test]
fn tunnels_the_game_refuses_are_errors() {
    // Observed 2026-11-15: the game reports these as compile errors.
    let error = |art: &str| Components::build(&textboard::parse(&format!("\n{art}")).unwrap().board).err().unwrap();
    assert!(error("2T..T9").contains("Tunnel at 17,16: no exit with the same ink (left to right)"));
    assert!(error("5T5").contains("Tunnel at 17,16: no exit with the same ink"));
    assert!(error("2T.2T.T2").contains("another entrance for the same ink at 20,16"));
    assert!(Components::build(&textboard::parse("\n2T.9T.T2.T9").unwrap().board).is_ok(), "pairs of different colours may interleave");
}

#[test]
fn touching_crosses_are_an_error() {
    let tb = textboard::parse("\n5++5").unwrap();
    assert!(Components::build(&tb.board).err().unwrap().contains("Cross at 17,16"));
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
