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
        // A board with Timer ink states how long a timer interval is for the engine run.
        let timer_ticks = field(&checklist, "- timer ticks:").map(|n| n.parse().unwrap());
        let mut engine = Engine::new(&tb.board, components, &vcb::sim::Options { timer_ticks }).unwrap();
        let mut differences = Vec::new();
        // Boards without probes (VMem recordings) have no ticks to compare here.
        engine.run_until(&StopSet { max_ticks: tb.ticks.last().copied().unwrap_or(0), breakpoint: false }, |e| {
            let Some((_, lit)) = observed.iter().find(|(t, _)| *t == e.tick()) else { return };
            for (i, entity) in entities.iter().enumerate() {
                let (number, engine_says) = (i + 1, e.state()[*entity as usize]);
                if !ignored.contains(&number) && engine_says != lit.contains(&number) {
                    differences.push(format!("tick {}: LED {number} ({}) is {} in the engine", e.tick(), tb.probes[i].label, if engine_says { "on" } else { "off" }));
                }
            }
        });
        assert!(differences.is_empty(), "{name} disagrees with the game:\n{}", differences.join("\n"));

        // Observed on `general`: when a Breakpoint pauses the game, its counter shows one more than
        // the tick on which the Breakpoint turned on, which is the engine's stop tick.
        if let Some(Ok(counter)) = field(&checklist, "- Tick counter when a Breakpoint paused the game (if any):").map(str::parse::<u64>) {
            let mut engine = Engine::new(&tb.board, Components::build(&tb.board).unwrap(), &Default::default()).unwrap();
            let report = engine.run_until(&StopSet { max_ticks: counter + 10, breakpoint: true }, |_| {});
            assert_eq!((report.tick + 1, report.reasons), (counter, vec!["breakpoint"]), "{name}: Breakpoint pause");
        }
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

#[test]
fn every_ink_pair_touching_in_the_cpu_is_on_an_observed_board() {
    let mut observed = BTreeSet::new();
    for name in fixtures() {
        observed.extend(vcb::adjacency(&Board::read(&fixture_file(&name, "vcb")).unwrap()).into_keys());
    }
    let cpu = Board::read(&engine_dir().join("../32-bit-alu-try.vcb")).unwrap();
    let missing: Vec<_> = vcb::adjacency(&cpu).into_keys().filter(|pair| !observed.contains(pair)).collect();
    assert!(missing.is_empty(), "the CPU has touching inks that no observed board covers: {missing:?}");
}

#[test]
fn recorded_random_words_are_fair_coin_flips() {
    // The game's own Random output, stored through VMem: 127 words of 32 bits per seed (words 1..=127).
    // This pins down what was observed; the engine's Random is checked for the same properties in model.rs.
    for seed in [1, 2] {
        let name = format!("random-vmem-seed-{seed}");
        let before = Board::read(&fixture_file(&name, "vcb")).unwrap();
        let after = Board::read(&fixture_file(&name, "after.vcb")).unwrap();
        assert!(before.inks == after.inks, "{name}: the run was made on another board");
        let words = &after.vmem[1..128];
        assert!(words.iter().all(|w| *w != 0) && after.vmem[128..].iter().all(|w| *w == 0), "{name}: words 1..=127 are filled, nothing after");
        let ones: u32 = words.iter().map(|w| w.count_ones()).sum();
        assert!((1900..=2164).contains(&ones), "{name}: {ones} ones in 4064 bits");
        // A bit that was on is on again in the next word about half the time: every draw is independent.
        let repeats: u32 = words.windows(2).map(|w| (w[0] & w[1]).count_ones()).sum();
        assert!((900..=1100).contains(&repeats), "{name}: {repeats} repeated ones");
    }
    let (a, b) = (Board::read(&fixture_file("random-vmem-seed-1", "after.vcb")).unwrap(), Board::read(&fixture_file("random-vmem-seed-2", "after.vcb")).unwrap());
    assert!(a.vmem != b.vmem, "the two seeds gave the same words");
}

#[test]
fn engine_reproduces_the_recorded_random_bits_exactly() {
    // The 32 Randoms sit three pixels above the content pins and are triggered every 24 ticks, first at tick 2.
    // Word c of the game's save holds their outputs after trigger c. The engine must give the same 4064 bits.
    for seed in [1, 2] {
        let name = format!("random-vmem-seed-{seed}");
        let board = Board::read(&fixture_file(&name, "vcb")).unwrap();
        let recorded = Board::read(&fixture_file(&name, "after.vcb")).unwrap().vmem;
        let components = Components::build(&board).unwrap();
        let s = board.settings.vmem_settings;
        let randoms: Vec<u32> = (0..32).map(|k| components.entity_at((s[8] - s[10] * k) as usize, s[9] as usize - 3).unwrap()).collect();
        let mut engine = Engine::new(&board, components, &Default::default()).unwrap();
        let mut wrong = Vec::new();
        engine.run_until(&StopSet { max_ticks: 2 + 24 * 127, breakpoint: false }, |e| {
            let (cycle, phase) = ((e.tick() as usize).saturating_sub(2) / 24, (e.tick() + 22) % 24);
            if phase == 0 && (1..=127).contains(&cycle) {
                let word: u32 = randoms.iter().enumerate().map(|(k, r)| (e.state()[*r as usize] as u32) << k).sum();
                if word != recorded[cycle] {
                    wrong.push(format!("word {cycle}: engine {word:08x}, game {:08x}", recorded[cycle]));
                }
            }
        });
        assert!(wrong.is_empty(), "{name}: {} of 127 words differ, first: {}", wrong.len(), wrong[0]);
    }
}
