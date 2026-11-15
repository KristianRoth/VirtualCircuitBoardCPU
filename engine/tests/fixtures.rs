//! Observed fixtures: boards the user stepped in the game, with the checklist they filled in.
//! Each fixture is tests/boards/<name>.vcb + <name>.observed.md, generated from boards/<name>.txt.

use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use vcb::netlist::Components;
use vcb::sim::{Engine, Simulation, StopSet};
use vcb::{textboard, Board};

fn engine_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// The names of all fixtures, from the `.observed.md` files.
fn fixtures() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(engine_dir().join("tests/boards"))
        .unwrap()
        .filter_map(|entry| entry.unwrap().file_name().to_str()?.strip_suffix(".observed.md").map(String::from))
        .collect();
    names.sort();
    assert!(!names.is_empty());
    names
}

fn fixture_file(name: &str, extension: &str) -> PathBuf {
    engine_dir().join("tests/boards").join(format!("{name}.{extension}"))
}

/// The text after `prefix` on the first line that starts with it.
fn field<'a>(checklist: &'a str, prefix: &str) -> Option<&'a str> {
    checklist.lines().find_map(|line| line.strip_prefix(prefix)).map(str::trim)
}

fn numbers(list: &str) -> BTreeSet<usize> {
    list.split(',').map(|n| n.trim().parse().unwrap_or_else(|_| panic!("\"{list}\" is not a list of LED numbers"))).collect()
}

/// (tick, lit LED numbers) for every observation line. `same` repeats the line before.
fn observations(checklist: &str) -> Vec<(u64, BTreeSet<usize>)> {
    let mut out: Vec<(u64, BTreeSet<usize>)> = Vec::new();
    for line in checklist.lines() {
        let Some((tick, lit)) = line.strip_prefix("- tick ").and_then(|rest| rest.split_once(", lit LEDs:")) else { continue };
        let lit = match lit.trim() {
            "none" => BTreeSet::new(),
            "same" => out.last().expect("`same` needs a line before it").1.clone(),
            list => numbers(list),
        };
        out.push((tick.parse().unwrap(), lit));
    }
    out
}

#[test]
fn engine_matches_every_observation() {
    for name in fixtures() {
        let checklist = std::fs::read_to_string(fixture_file(&name, "observed.md")).unwrap();
        let source = std::fs::read_to_string(engine_dir().join("boards").join(format!("{name}.txt"))).unwrap();
        let tb = textboard::parse(&source).unwrap();
        let ignored = field(&checklist, "- ignore LEDs:").map(numbers).unwrap_or_default();
        let observed = observations(&checklist);
        assert_eq!(observed.iter().map(|(t, _)| *t).collect::<Vec<_>>(), tb.ticks, "{name}: every listed tick needs an observation");

        let components = Components::build(&tb.board).unwrap();
        let entities: Vec<u32> = tb.probes.iter().map(|p| components.entity_at(p.x, p.y).unwrap()).collect();
        let mut engine = Engine::new(&tb.board, components).unwrap();
        let mut differences = Vec::new();
        engine.run_until(&StopSet { max_ticks: *tb.ticks.last().unwrap(), breakpoint: false }, |e| {
            let Some((_, lit)) = observed.iter().find(|(t, _)| *t == e.tick()) else { return };
            for (i, entity) in entities.iter().enumerate() {
                let (number, engine_says) = (i + 1, e.state()[*entity as usize]);
                if !ignored.contains(&number) && engine_says != lit.contains(&number) {
                    differences.push(format!("tick {}: LED {number} ({}) is {} in the engine", e.tick(), tb.probes[i].label, if engine_says { "on" } else { "off" }));
                }
            }
        });
        assert!(differences.is_empty(), "{name} disagrees with the game:\n{}", differences.join("\n"));
    }
}

#[test]
fn fixture_boards_are_the_boards_that_were_observed() {
    for name in fixtures() {
        let checklist = std::fs::read_to_string(fixture_file(&name, "observed.md")).unwrap();
        let vcb_text = std::fs::read_to_string(fixture_file(&name, "vcb")).unwrap();
        let hash = format!("`{:x}`", Sha256::digest(vcb_text.as_bytes()));
        assert_eq!(field(&checklist, "- Board SHA-256:"), Some(hash.as_str()), "{name}: the .vcb is not the file the checklist was made for");
        assert_eq!(field(&checklist, "- Game version:"), Some(vcb::ink::GAME_VERSION), "{name}");

        // The text-art source still produces the observed board.
        let source = std::fs::read_to_string(engine_dir().join("boards").join(format!("{name}.txt"))).unwrap();
        let generated = textboard::parse(&source).unwrap().board;
        let observed = Board::parse(&vcb_text, || unreachable!()).unwrap();
        assert!(generated.inks == observed.inks && generated.settings == observed.settings, "{name}: boards/{name}.txt no longer matches the observed .vcb");
    }
}

#[test]
fn every_verified_ink_is_on_an_observed_board() {
    let mut seen = BTreeSet::new();
    for name in fixtures() {
        let board = Board::read(&fixture_file(&name, "vcb")).unwrap();
        seen.extend(board.inks.iter().filter_map(|ink| ink.unit()));
    }
    for unit in vcb::ink::VERIFIED.iter().chain(vcb::ink::MODELLED) {
        assert!(seen.contains(unit), "{unit} is marked verified but is on no observed board");
    }
}
