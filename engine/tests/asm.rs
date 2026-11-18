//! The assembler against the game (the day 1 fixture) and against its gate (specs/vcb-assembler.md).

use std::path::PathBuf;
use vcb::asm::assemble;
use vcb::Board;

fn fixture(name: &str) -> Board {
    Board::read(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/boards").join(name)).unwrap()
}

/// A board with VMem on: 16 address bits and `content_bits` content bits.
fn board(content_bits: i64) -> Board {
    let mut board = Board::new(40, 4);
    board.settings.vmem_enabled = true;
    // Address pins along row 0 and content pins along row 2 (leftwards from x = 39), so the board also runs.
    board.settings.vmem_settings = [16, 39, 0, 1, 0, 1, 1, content_bits, 39, 2, 1, 0, 1, 1, 0, 0];
    board
}

/// Assembles on a 32-bit board. Ok: the non-zero words as (address, value) in address order.
fn asm(source: &str, allow: bool) -> Result<Vec<(usize, u32)>, String> {
    let mut b = board(32);
    assemble(source, "t", &mut b, allow)?;
    Ok(b.vmem.iter().enumerate().filter(|(_, w)| **w != 0).map(|(a, w)| (a, *w)).collect())
}

/// The game assembled this text (the after-save's Assembly Editor field) into the memory of `cpu-aoc1p1.vcb`.
#[test]
fn day_1_part_1_assembles_to_the_games_memory() {
    let source = fixture("cpu-aoc1p1.after.vcb").assembly;
    let expected = fixture("cpu-aoc1p1.vcb");
    let mut board = fixture("cpu-32bit-manual-start.vcb");
    let report = assemble(&source, "aoc1p1", &mut board, false).unwrap();
    let wrong = (0..board.vmem.len()).filter(|a| board.vmem[*a] != expected.vmem[*a]).count();
    assert_eq!(wrong, 0, "words that differ from the game's memory");
    assert!(report.unverified.is_empty());
    assert_eq!(report.replaced, 0);
    assert_eq!(board.vmem[1], 0x04050607, "words the program does not define are kept");

    let listing = report.listing();
    assert!(listing.starts_with("0x00100 0x00000000  "), "{}", &listing[..60]);
    assert!(listing.lines().next().unwrap().ends_with("pointer count 0x0100 0"));
    assert!(listing.lines().any(|l| l.starts_with("0x0FF00 0x00003032  ") && l.ends_with("  lia 50")));
    for entry in ["0x00100  count", "0x00101  input", "0x0FF02  LOOP", "0x0FF3B  END"] {
        assert!(listing.lines().any(|l| l == entry), "the listing lacks {entry:?}");
    }

    // Assembling over a board that already holds the program replaces nothing different; over part 1's
    // result with a changed program it says how many words it replaced.
    let mut again = fixture("cpu-aoc1p1.vcb");
    assert_eq!(assemble(&source, "aoc1p1", &mut again, false).unwrap().replaced, 0);
    let changed = source.replace("lia 50;", "lia 51;");
    assert_eq!(assemble(&changed, "aoc1p1", &mut again, false).unwrap().replaced, 1);
}

/// Day 1 part 2: the game's save made straight after entering simulation holds the assembled program untouched
/// (Persistent Memory 0-65535). It shows `@ name;`, an indented label, and the words both pointers store.
#[test]
fn day_1_part_2_assembles_to_the_games_memory() {
    let game = fixture("cpu-aoc1p2.assembled.vcb");
    let mut board = fixture("cpu-aoc1p2.vcb");
    assert!(board.inks == game.inks && board.assembly == game.assembly);
    assert!(std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/boards/cpu-aoc1p2.asm-observed.md")).unwrap().contains("Observed by: "));
    let report = assemble(&game.assembly, "aoc1p2", &mut board, false).unwrap();
    assert!(report.unverified.is_empty());
    let wrong = (1..=65535).filter(|a| board.vmem[*a] != game.vmem[*a]).count();
    assert_eq!(wrong, 0, "words that differ from the game's memory");
    assert_eq!((game.vmem[0x100], game.vmem[0x101]), (0, 0x1000), "the pointer words, straight from the game");
    assert!(report.names.iter().any(|(n, _)| n == "SKIP_DEC") && report.names.iter().any(|(n, _)| n == "SKIP_INC"));
}

/// Part 2 run by the engine on the CPU with its run switch on. 5831 is the accepted Advent of Code answer.
///   cargo test --release -- --ignored
#[test]
#[ignore]
fn day_1_part_2_runs_to_the_right_answer() {
    use vcb::netlist::Components;
    use vcb::sim::{Engine, Simulation, StopSet};
    let mut board = fixture("cpu-aoc1p1.vcb");
    board.vmem = fixture("cpu-32bit-manual-start.vcb").vmem;
    assemble(&fixture("cpu-aoc1p2.assembled.vcb").assembly, "aoc1p2", &mut board, false).unwrap();
    let mut engine = Engine::new(&board, Components::build(&board).unwrap(), &Default::default()).unwrap();
    let report = engine.run_until(&StopSet { max_ticks: 500_000_000, breakpoint: true, ..Default::default() }, |_| {});
    assert_eq!(report.reasons, vec!["breakpoint"]);
    assert_eq!(engine.vmem()[0x100], 0x16C7, "count");
}

/// Forms the game refused on the probe boards.
#[test]
fn what_the_game_rejects_is_refused() {
    let cases = [
        "origin orgprev\n1",
        "origin orgbase\n1",
        "origin 0x10\n0x1_0000_0000",
        "origin 0x10\n0xFFFFFFFF + 1",
        "origin 0x10\n0x10000 * 0x10000",
        "origin 0x10\n0 - 0x80000001",
        "origin 0x10\n-0x80000001",
        "origin 0x10\n0x80000000 << 1",
        "origin 0x10\n1 << 32",
        "origin 0x10\n1 << 1000",
        "origin 0x10\n8 >> -1",
        "origin 0x10\n6 / 0",
        "origin 0x10\n6 % 0",
        "symbol a 1; symbol a 2",
        "pointer p 0x20 1; pointer p 0x21 1",
        "macro m { a? | b? }\nmacro m { a? | b? }",
        "origin 0x10\n@ L\n1\n@ L\n2",
        "symbol a 1; pointer a 0x20 1",
        "symbol a 1\norigin 0x10\n@ a\n1",
        "symbol sub_bookmark 1",
        "origin 0x10\nnope",
        "origin 0x10\nsymbol a 1; unsymb a; a",
        "pointer p 0x20 1; unpoint p\norigin 0x10\np",
        "origin 0x10\na; symbol a 1",
        "origin 0x10\np; pointer p 0x20 1",
        "unsymb a",
        "unpoint p",
        "unmac m",
        "origin 0x10\n1 inline",
        "macro m { a? | b? }\norigin 0x10\nm 1",
        "macro m { a? | b? }\norigin 0x10\nm 1 2 4",
        "macro m { a? | b? }\norigin 0x10\nm ( 1 | 2 ) 4",
        "macro m { a? | b? }\norigin 0x10\n1 m",
        "macro m { a? | c? }",
        "macro n { a? | b? }\nmacro m { n | a? | b? }\norigin 0x10\nm 1 2",
        "origin 0x10\nmacro m {\na? | b?\n}",
        "macro m { a? | b? }; origin 0x10",
        "origin 0x10\n1|2",
        "origin 0x10\n@ L 5",
        "Symbol a 1",
        "origin 0x10\n0X10",
        "symbol a.b 1",
        "symbol 1a 1",
        "origin 0x10\n1 2 | 4",
        "resymb a 1",
        "symbol a 1; repoint a 0x20 2",
        "symbol a 0x10; origin a",
        "symbol a 0x20; pointer p a 1",
        "origin 0x100000",
        "origin 0",
        "pointer p 0 1",
        "pointer p 0x100000 1",
        "origin 0x10\r\n5",
        "origin 0x10\n1\n@ L",
    ];
    for source in cases {
        for allow in [false, true] {
            let refused = asm(source, allow).expect_err(source);
            assert!(refused.contains("the game rejects this"), "{source:?}: {refused}");
        }
    }
    // A symbol whose value is an expression has too many tokens; a parenthesis left open cannot be parsed.
    assert!(asm("symbol a 1 + 1", true).is_err() && asm("origin 0x10\n(1", true).is_err());
}

/// The few cases of facts question 23 that are still open: always a gate hit naming the letter.
#[test]
fn undefined_cases_are_always_gate_hits() {
    let cases: &[(&str, &str)] = &[
        ("b", "origin 0x10\n(0xFFFFFFFF + 1) - 1"),
        ("c", "origin 0x10\n0 << 64"),
        ("c", "origin 0x10\n1 >> 64"),
        ("n", "origin 0xFFFFF\n1; 2"),
        ("p", "symbol a L\norigin 0x10\n@ L\n1"),
        ("p", "pointer p 0x20 1; pointer q 0x21 p"),
        ("u", "origin 0x10\n~ ~ 5"),
        ("u", "origin 0x10\n- - 5"),
    ];
    for (letter, source) in cases {
        for allow in [false, true] {
            let refused = asm(source, allow).expect_err(source);
            assert!(refused.contains(&format!("(facts question 23{letter})")), "23{letter} {source:?}: {refused}");
        }
    }
}

/// Rules the second probe board showed, as the game's own results.
#[test]
fn arithmetic_and_macros_follow_the_game() {
    let cases: &[(&str, u32)] = &[
        ("1 | 6 ^ 7 & 12 << 1 + 2 * 3", 7),
        ("64 / 4 / 2", 8),
        ("100 % 7 % 3", 2),
        ("1 << 2 << 1", 8),
        ("-1 + 2", 1),
        ("-1 1", 0xFFFF_FFFF),
        ("-2 * 3", 0xFFFF_FFFA),
        ("-8 >> 1", 0xFFFF_FFFC),
        ("-7 / 2", 0xFFFF_FFFD),
        ("-7 % 3", 0xFFFF_FFFF),
        ("~ -1 | 0x40", 0x40),
        ("~ 1 + 1", 0xFFFF_FFFF),
        ("~ (1 + 1)", 0xFFFF_FFFD),
        ("- (1 + 1)", 0xFFFF_FFFE),
        ("0xFFFF * 0x10000", 0xFFFF_0000),
        ("0x80000000 >> 31", 1),
        ("0x80000000 >> 33 | 0x40", 0x40),
        ("0 << 32 | 0x40", 0x40),
        ("-0x80000000", 0x8000_0000),
    ];
    for (expression, word) in cases {
        assert_eq!(asm(&format!("origin 0x10\n{expression}"), false).unwrap_or_else(|e| panic!("{expression}: {e}")), [(0x10, *word)], "{expression}");
    }
    // A macro name takes the tokens after it as arguments and is replaced by its body, wherever it stands.
    let macros = "macro pair { a? | b? }\nmacro none { 1 | 2 }\nmacro one {a?}\nmacro sum { a? + b? * 2 }\norigin 0x10\n";
    assert_eq!(asm(&format!("{macros}pair 1 2 | 4; none; one 5; sum 1 2; 8 | pair 1 2"), false).unwrap(), [(0x10, 7), (0x11, 3), (0x12, 5), (0x13, 5), (0x14, 11)]);
    // A body of several statements, with labels and a placeholder glued to a name, as in the game's compact
    // sample computer: `printb 0` there stores `lia StringStart0; psh; cal PrintStringMem; jmp StringEnd0`.
    let multi = "macro begin {Starta?; 7; @ Starta?; Enda?}\nmacro end {9; @ Enda?}\norigin 0x10\nbegin 0; 5; end 0; 6; begin x; end x; 3";
    assert_eq!(asm(multi, false).unwrap(), [(0x10, 0x12), (0x11, 7), (0x12, 0x15), (0x13, 5), (0x14, 9), (0x15, 6), (0x16, 0x18), (0x17, 7), (0x18, 0x1A), (0x19, 9), (0x1A, 3)]);
    for (source, message) in [
        ("macro two {1; 2}; 3", "a ; on the line of a macro definition"),
        ("macro two {a?; 2}\norigin 0x10\ntwo", "takes 1 arguments, not 0"),
        ("macro two {1; 2}\norigin 0x10\n4 | two", "used inside an expression"),
        ("macro two {1; origin 5}\norigin 0x10\ntwo", "a directive inside a macro body"),
        ("macro one {1}\nmacro two {one; 2}\norigin 0x10\ntwo", "a macro name inside a macro body"),
    ] {
        assert!(asm(source, false).unwrap_err().contains(message), "{source}");
    }
    // A label names the next instruction, also past an inline pointer; repoint can be inline.
    assert_eq!(asm("origin 0x10\n@ L; pointer p inline 5; 1; L; repoint p inline 9; p", false).unwrap(), [(0x10, 5), (0x11, 1), (0x12, 0x11), (0x13, 9), (0x14, 0x13)]);
    // A symbol may be valued by a pointer or by a label defined above it; a pointer by a symbol.
    assert_eq!(asm("pointer p 0x30 7; symbol a p\norigin 0x10\n@ L; 1; symbol b L; a; b; pointer q 0x31 b", false).unwrap(), [(0x10, 1), (0x11, 0x30), (0x12, 0x10), (0x30, 7), (0x31, 0x10)]);
}

#[test]
fn nothing_outside_the_rules_assembles() {
    for source in ["origin 0x10\n1 ? 2", "symbol", "macro m {", "@", "pointer p", "origin 0x10\n(1", "origin 0x10\n1 |", "origin 0x10\n0b102", "origin 0x10\n1 2 | 3"] {
        assert!(asm(source, true).is_err(), "{source:?} assembled");
    }
    let deep = format!("origin 0x10\n{}1{}", "(".repeat(65), ")".repeat(65));
    assert!(asm(&deep, true).unwrap_err().contains("nested more than 64"));
    let mut b = board(32);
    assert!(assemble(&" ".repeat((16 << 20) + 1), "t", &mut b, true).unwrap_err().contains("16 MiB"));
    assert!(assemble("origin 0x10\n\x1b[2J", "t", &mut b, true).unwrap_err().contains("control character"));
    b.settings.vmem_enabled = false;
    assert!(assemble("origin 0x10\n1", "t", &mut b, true).unwrap_err().contains("is_vmem_enabled"));
}

#[test]
fn every_gate_hit_is_listed_and_a_real_error_stops() {
    // Two rejected forms and an open question, and no real error: all three are listed, and the board is unchanged.
    let mut b = board(32);
    let error = assemble("origin 0x10\nnope\n~ ~ 5\n6 / 0", "t", &mut b, false).unwrap_err();
    let lines: Vec<&str> = error.lines().collect();
    assert_eq!(lines.len(), 3, "{error}");
    assert!(lines.iter().any(|l| l.starts_with("t:2:1: ")) && lines.iter().any(|l| l.starts_with("t:3:1: ")) && lines.iter().any(|l| l.starts_with("t:4:1: ")), "{error}");
    assert!(b.vmem.iter().all(|w| *w == 0));

    // A gate hit, then a real error: both, the gate hit first; nothing after the real error.
    let error = asm("origin 0x10\n1 inline\n(1\nnope", false).unwrap_err();
    let lines: Vec<&str> = error.lines().collect();
    assert_eq!(lines.len(), 2, "{error}");
    assert!(lines[0].contains("a keyword inside an instruction") && lines[1].starts_with("t:3:1: "), "{error}");

    // Positions: after a `;`, and inside a macro use.
    assert!(asm("origin 0x10\n1; foo", false).unwrap_err().starts_with("t:2:4: "));
    let error = asm("symbol x 1\nmacro m { a? | b? | nope }\norigin 0x10\nm x x", false).unwrap_err();
    assert!(error.contains("t:4:1: ") && error.contains("(in macro m)"), "{error}");
}

#[test]
fn empty_sources_and_word_0() {
    for source in ["", " \n\t\n", "symbol a 1", "# only a comment; with a semicolon\n"] {
        let mut b = board(32);
        let report = assemble(source, "t", &mut b, false).unwrap();
        assert!(report.words.is_empty() && b.vmem.iter().all(|w| *w == 0), "{source:?}");
    }
    assert_eq!(assemble("symbol a 1", "t", &mut board(32), false).unwrap().summary("t"), "assembled 0 words from t");

    // The content bits do not limit a stored word: the game stores all 32 bits on a 16-bit board too.
    let mut narrow = board(16);
    narrow.vmem[0x10] = 0xABCD_0000;
    assemble("origin 0x10\n0xFFFF; 0x12345; -1", "t", &mut narrow, false).unwrap();
    assert_eq!(narrow.vmem[0..0x13], [&[0u32; 0x10][..], &[0xFFFF, 0x12345, 0xFFFF_FFFF]].concat());
}

// ---- the probe boards: what the game assembled and what it rejected (milestone A3) ----

/// (text before the first case, the cases as (id, lines)).
fn cases(program: &str) -> (String, Vec<(String, Vec<String>)>) {
    let mut prelude = String::new();
    let mut cases: Vec<(String, Vec<String>)> = Vec::new();
    for line in program.lines() {
        match (line.strip_prefix("# case "), cases.last_mut()) {
            (Some(id), _) => cases.push((id.trim().to_string(), Vec::new())),
            (None, Some((_, lines))) if !line.trim().is_empty() => lines.push(line.to_string()),
            (None, None) => prelude += &format!("{line}\n"),
            _ => {}
        }
    }
    (prelude, cases)
}

#[test]
fn the_probe_saves_match_the_game() {
    let observed = |name: &str| std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("tests/boards/{name}.asm-observed.md"))).unwrap();
    // A case the game accepted whose result the save cannot show stays an open question; it is cut from the text.
    let still_open = ["u-not-not", "n-past-the-last-word"];
    for name in ["asm-probe-32", "asm-probe-16", "asm-reject", "asm-probe2"] {
        let (board, game) = (fixture(&format!("{name}.vcb")), fixture(&format!("{name}.assembled.vcb")));
        // The asm-reject save has one more Write pixel than its board (drawn by accident during the session).
        let stray_pixels = board.inks.iter().zip(&game.inks).filter(|(a, b)| a != b).count();
        assert!(stray_pixels == (name == "asm-reject") as usize && board.settings.vmem_settings == game.settings.vmem_settings, "{name}: another board");
        let checklist = observed(name);
        assert!(checklist.lines().any(|l| l.starts_with("- Observed by: ") && l.len() > 20), "{name}: who observed it?");

        // What the game left in memory is what the engine assembles from the text the game was left with.
        let (prelude, saved) = cases(&game.assembly);
        let text = saved.iter().filter(|(id, _)| !still_open.contains(&id.as_str())).fold(prelude.clone(), |text, (id, lines)| text + &format!("# case {id}\n{}\n", lines.join("\n")));
        let mut assembled = board.clone();
        let report = assemble(&text, name, &mut assembled, false).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(report.unverified.is_empty());
        let (from, to) = (1, game.settings.vmem_settings[15] as usize);
        // The words of the still-open cases: `~ ~ 5` (stored -5) and the last VMem word, written by both `n-` cases.
        let unexplained = |a: &usize| name == "asm-probe2" && [0x380, 0x381, 0xFFFFF].contains(a);
        let wrong: Vec<String> = (from..=to).filter(|a| assembled.vmem[*a] != game.vmem[*a] && !unexplained(a)).map(|a| format!("{a:#X}: engine {:#X}, game {:#X}", assembled.vmem[a], game.vmem[a])).collect();
        assert!(wrong.is_empty(), "{name}: {wrong:?}");

        // Every case the user had to comment out is refused by the engine too, and its message is on record.
        let (_, original) = cases(&board.assembly);
        let mut rejected = 0;
        for ((id, before), (_, after)) in original.iter().zip(&saved) {
            let was_live = before.iter().any(|l| !l.trim_start().starts_with('#'));
            if was_live && after.iter().all(|l| l.trim_start().starts_with('#')) {
                rejected += 1;
                let alone = format!("{prelude}{}\n", before.join("\n"));
                assert!(assemble(&alone, id, &mut board.clone(), true).is_err(), "{name}: the game rejects {id}, the engine does not");
                assert!(checklist.lines().any(|l| l.starts_with(&format!("- {id}: ")) && l.len() > id.len() + 6), "{name}: no message for {id}");
            }
        }
        assert_eq!(rejected, match name { "asm-reject" => 36, "asm-probe2" => 19, _ => 7 }, "{name}");
    }

    // External Editing: the game assembled the CRLF file beside the board, not the board's own text.
    let (board, game) = (fixture("asm-external.vcb"), fixture("asm-external.assembled.vcb"));
    let file = std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/boards/asm-external.vcbasm")).unwrap();
    assert!(board.assembly_is_external && board.inks == game.inks && file.contains("\r\n") && !board.assembly.contains("0xE7E1"));
    let mut assembled = board.clone();
    assemble(&file.replace("\r\n", "\n"), "asm-external", &mut assembled, false).unwrap();
    assert!((1..=8191).all(|a| assembled.vmem[a] == game.vmem[a]) && game.vmem[0x1010] == 0xE7E1 && game.vmem[0x1000] == 0);
    assert!(observed("asm-external").contains("Observed by: K") && observed("asm-novmem").contains("simulation start in the end (yes/no): yes"));

    // A carriage return in the editor text: the game showed an error and did not start.
    let crfield = fixture("asm-crfield.vcb");
    assert!(crfield.assembly.contains("\r\n") && observed("asm-crfield").contains("INVALID_TOKEN"));
    assert!(assemble(&crfield.assembly, "asm-crfield", &mut crfield.clone(), true).unwrap_err().contains("the game rejects this"));
    // External Editing on and no file: "External assembly file not found" (the engine's side is a command-line test).
    assert!(fixture("asm-external-missing.vcb").assembly_is_external && observed("asm-external-missing").contains("External assembly file not found"));
}

#[test]
fn the_assembly_fields_survive_a_round_trip() {
    let mut b = board(32);
    b.assembly = "symbol a 1\n\t# é \"quoted\"\n".to_string();
    b.assembly_is_external = true;
    let again = Board::parse(&b.to_vcb(), || unreachable!()).unwrap();
    assert_eq!((again.assembly.as_str(), again.assembly_is_external), (b.assembly.as_str(), true));

    // A file without the External Editing key reads as false.
    let text = b.to_vcb().replace("\"assembly_is_external\": true,", "");
    assert!(!Board::parse(&text, || unreachable!()).unwrap().assembly_is_external);
}

// ---- the `vcb asm` command ----

fn dir(name: &str) -> PathBuf {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn vcb(args: &[&str]) -> std::process::Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_vcb")).args(args).output().unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).unwrap()
}

/// Writes a 32-bit board with `assembly` as its Assembly Editor text; returns its path.
fn write_board(d: &std::path::Path, assembly: &str) -> String {
    let mut b = board(32);
    b.assembly = assembly.to_string();
    let path = d.join("b.vcb");
    std::fs::write(&path, b.to_vcb()).unwrap();
    path.to_str().unwrap().to_string()
}

#[test]
fn vcb_asm_writes_a_board_a_dump_and_a_listing() {
    let d = dir("asm-outputs");
    let b = write_board(&d, "origin 0x10\n1");
    let p = |name: &str| d.join(name).to_str().unwrap().to_string();
    std::fs::write(p("prog.vcbasm"), "symbol lia 0x3000\r\norigin 0x20\r\n@ START\r\nlia 50\r\nSTART\r\n").unwrap();
    let before = (std::fs::read(&b).unwrap(), std::fs::read(p("prog.vcbasm")).unwrap());

    // --asm wins over the board's own text; CRLF in the file is read as LF.
    let out = vcb(&["asm", &b, "--asm", &p("prog.vcbasm"), "-o", &p("new.vcb"), "--dump-vmem", &p("mem.txt"), "--listing", &p("list.txt")]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), format!("assembled 2 words from {}\n", p("prog.vcbasm")));
    assert_eq!(text(&std::fs::read(p("list.txt")).unwrap()), "0x00020 0x00003032  4  lia 50\n0x00021 0x00000020  5  START\n\n0x00020  START\n");
    let dump = text(&std::fs::read(p("mem.txt")).unwrap());
    assert_eq!(dump.lines().count(), 1 << 20);
    assert!(dump.contains("0x00000020 0x00003032\n0x00000021 0x00000020\n") && dump.contains("0x00000010 0x00000000\n"));

    // The written board holds the memory and the program (LF), and assembles to the same memory again.
    let new = Board::read(&d.join("new.vcb")).unwrap();
    assert_eq!((new.vmem[0x20], new.vmem[0x21], new.vmem[0x10]), (0x3032, 0x20, 0));
    assert_eq!(new.assembly, "symbol lia 0x3000\norigin 0x20\n@ START\nlia 50\nSTART\n");
    let out = vcb(&["asm", &p("new.vcb"), "--dump-vmem", &p("mem2.txt")]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(std::fs::read(p("mem.txt")).unwrap(), std::fs::read(p("mem2.txt")).unwrap());

    // Without --asm the board's own text is used; "-" prints the listing.
    let out = vcb(&["asm", &b, "--listing", "-"]);
    assert_eq!(text(&out.stdout), format!("assembled 1 words from {b}:assembly\n0x00010 0x00000001  2  1\n\n"));

    // The inputs are never changed.
    assert_eq!(before, (std::fs::read(&b).unwrap(), std::fs::read(p("prog.vcbasm")).unwrap()));
}

#[test]
fn vcb_asm_never_overwrites_an_input_or_leaves_half_a_result() {
    let d = dir("asm-safety");
    let b = write_board(&d, "origin 0x10\n1");
    let p = |name: &str| d.join(name).to_str().unwrap().to_string();
    std::fs::write(p("prog.vcbasm"), "origin 0x10\n2\n").unwrap();
    std::fs::write(p("b.vcbasm"), "origin 0x10\n3\n").unwrap();
    let fails = |args: &[&str], needle: &str| {
        let out = vcb(args);
        let stderr = text(&out.stderr);
        assert!(!out.status.success() && stderr.contains(needle), "{args:?}: {stderr}");
    };
    for flag in ["-o", "--dump-vmem", "--listing"] {
        fails(&["asm", &b, "--asm", &p("prog.vcbasm"), flag, &p("prog.vcbasm"), "--force"], "is an input file");
        fails(&["asm", &b, flag, &b, "--force"], "is an input file");
        fails(&["asm", &b, flag, &p("b.vcbasm"), "--force"], "is an input file");
    }
    std::fs::write(p("old.txt"), "keep").unwrap();
    fails(&["asm", &b, "--listing", &p("old.txt")], "already exists");
    assert!(vcb(&["asm", &b, "--listing", &p("old.txt"), "--force"]).status.success());

    // A program with a gate hit and a real error: both are printed, and no output file appears.
    std::fs::write(p("bad.vcbasm"), "nope\norigin 0x10\n(1\n").unwrap();
    let out = vcb(&["asm", &b, "--asm", &p("bad.vcbasm"), "-o", &p("never.vcb"), "--listing", &p("never.txt")]);
    let stderr = text(&out.stderr);
    assert!(!out.status.success() && stderr.contains("bad.vcbasm:1:1: the name 'nope' must be defined") && stderr.contains("bad.vcbasm:3:1: "), "{stderr}");
    assert!(!d.join("never.vcb").exists() && !d.join("never.txt").exists() && !d.join("never.vcb.part").exists());

    std::fs::write(p("latin1.vcbasm"), b"origin 0x10\n1 # \xff\n").unwrap();
    fails(&["asm", &b, "--asm", &p("latin1.vcbasm")], "latin1.vcbasm: is not valid UTF-8");
}

#[test]
fn vcb_asm_uses_the_external_file_when_external_editing_is_on() {
    let d = dir("asm-external");
    let mut b = board(32);
    b.assembly = "origin 0x10\n1".to_string();
    b.assembly_is_external = true;
    let path = d.join("x.vcb");
    std::fs::write(&path, b.to_vcb()).unwrap();
    let (board_path, listing) = (path.to_str().unwrap(), d.join("l.txt"));
    let run = || {
        let out = vcb(&["asm", board_path, "--listing", listing.to_str().unwrap(), "--force"]);
        (out.status.success(), text(&out.stdout) + &text(&out.stderr))
    };
    // Without the file the game shows "External assembly file not found".
    let (ok, said) = run();
    assert!(!ok && said.contains("x.vcbasm") && said.contains("the game rejects this"), "{said}");

    // With the file there, it is assembled and the board's own text is not (as the game does).
    std::fs::write(d.join("x.vcbasm"), "origin 0x10\r\n2\r\n").unwrap();
    let (ok, said) = run();
    assert!(ok && said.contains("assembled 1 words from") && said.contains("x.vcbasm") && !said.contains("unverified"), "{said}");
    assert_eq!(std::fs::read_to_string(&listing).unwrap(), "0x00010 0x00000002  2  2\n\n");
}

// ---- `vcb run` assembles before tick 0 ----

#[test]
fn vcb_run_assembles_the_boards_program_like_the_game() {
    let d = dir("run-asm");
    let b = write_board(&d, "origin 0x10\n1");
    let p = |name: &str| d.join(name).to_str().unwrap().to_string();
    std::fs::write(p("prog.vcbasm"), "origin 0x10\r\n2\r\n").unwrap();
    let before = std::fs::read(&b).unwrap();
    let run = |extra: &[&str]| {
        let mut args = vec!["run", b.as_str(), "--ticks", "1", "--watch", "0x10"];
        args.extend(extra);
        let out = vcb(&args);
        (out.status.success(), text(&out.stdout), text(&out.stderr))
    };
    let stopped = |value: u32| format!("stopped at tick 1: max-ticks\n0x00000010 = {value:#010X} ({value})\n");

    // The board's own text by default; --asm wins; --no-asm assembles nothing.
    assert_eq!(run(&[]), (true, format!("assembled 1 words from {b}:assembly\n{}", stopped(1)), String::new()));
    assert_eq!(run(&["--asm", &p("prog.vcbasm")]).1, format!("assembled 1 words from {}\n{}", p("prog.vcbasm"), stopped(2)));
    assert_eq!(run(&["--no-asm"]).1, stopped(0));
    let (ok, _, stderr) = run(&["--asm", &p("prog.vcbasm"), "--no-asm"]);
    assert!(!ok && stderr.contains("--asm and --no-asm cannot be used together"), "{stderr}");

    // The config file can name both; a command-line flag overrides it. Paths are relative to the working directory.
    std::fs::write(p("asm.toml"), format!("asm = {:?}\n", p("prog.vcbasm"))).unwrap();
    std::fs::write(p("no.toml"), "no-asm = true\n").unwrap();
    assert_eq!(run(&["--config", &p("asm.toml")]).1, format!("assembled 1 words from {}\n{}", p("prog.vcbasm"), stopped(2)));
    assert_eq!(run(&["--config", &p("asm.toml"), "--no-asm"]).1, stopped(0));
    assert_eq!(run(&["--config", &p("no.toml")]).1, stopped(0));
    assert_eq!(run(&["--config", &p("no.toml"), "--asm", &p("prog.vcbasm")]).1, format!("assembled 1 words from {}\n{}", p("prog.vcbasm"), stopped(2)));
    std::fs::write(p("both.toml"), format!("asm = {:?}\nno-asm = true\n", p("prog.vcbasm"))).unwrap();
    assert!(!run(&["--config", &p("both.toml")]).0);

    // A program the game would reject stops the run before tick 0.
    std::fs::write(p("bad.vcbasm"), "origin 0x10\nnope\n").unwrap();
    let (ok, stdout, stderr) = run(&["--asm", &p("bad.vcbasm")]);
    assert!(!ok && stdout.is_empty() && stderr.contains("bad.vcbasm:2:1: the name 'nope' must be defined before it is used: the game rejects this"), "{stderr}");

    // The program file cannot be an output, and no input is changed.
    let (ok, _, stderr) = run(&["--asm", &p("prog.vcbasm"), "--dump-vmem", &p("prog.vcbasm"), "--force"]);
    assert!(!ok && stderr.contains("is an input file"), "{stderr}");
    assert_eq!(before, std::fs::read(&b).unwrap());
    assert_eq!(std::fs::read(p("prog.vcbasm")).unwrap(), b"origin 0x10\r\n2\r\n");
}

#[test]
fn vcb_run_with_vmem_disabled_runs_without_assembling() {
    let d = dir("run-asm-novmem");
    let mut b = board(32);
    b.settings.vmem_enabled = false;
    b.assembly = "origin 0x10\n1".to_string();
    let path = d.join("b.vcb");
    std::fs::write(&path, b.to_vcb()).unwrap();
    let run = |extra: &[&str]| {
        let mut args = vec!["run", path.to_str().unwrap(), "--ticks", "1"];
        args.extend(extra);
        let out = vcb(&args);
        (out.status.success(), text(&out.stdout))
    };
    assert_eq!(run(&[]), (true, "assembly text not assembled: VMem is disabled\nstopped at tick 1: max-ticks\n".to_string()));
    assert_eq!(run(&["--no-asm"]), (true, "stopped at tick 1: max-ticks\n".to_string()));
    std::fs::write(d.join("p.vcbasm"), "origin 0x10\n1\n").unwrap();
    assert!(!run(&["--asm", d.join("p.vcbasm").to_str().unwrap()]).0);
}

// ---- `vcb gen` puts assembly programs on generated boards ----

#[test]
fn vcb_gen_puts_assembly_text_on_a_board() {
    let d = dir("gen-asm");
    let p = |name: &str| d.join(name).to_str().unwrap().to_string();
    let art = "\nLW5RB\n";
    std::fs::write(p("field.vcbasm"), "# case a-1\r\norigin 0x10\r\n1\r\n").unwrap();
    std::fs::write(p("ext.vcbasm"), "# case r-1\norigin 0x20\n2\n").unwrap();

    // Without the keys the board is byte-identical to one generated before the keys existed: no assembly.
    std::fs::write(p("plain.txt"), art).unwrap();
    assert!(vcb(&["gen", &p("plain.txt"), "-o", &p("plain.vcb")]).status.success());
    let plain = Board::read(&d.join("plain.vcb")).unwrap();
    assert!(plain.assembly.is_empty() && !plain.assembly_is_external && !d.join("plain.vcbasm").exists());
    assert!(std::fs::read_to_string(p("plain.checklist.md")).unwrap().starts_with("# Checklist: plain"));

    // assembly = <file>: the text (LF) in the board, and the assembler checklist with its cases.
    std::fs::write(p("a.txt"), format!("assembly = field.vcbasm\n{art}")).unwrap();
    let out = vcb(&["gen", &p("a.txt"), "-o", &p("a.vcb")]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let a = Board::read(&d.join("a.vcb")).unwrap();
    assert_eq!((a.assembly.as_str(), a.assembly_is_external), ("# case a-1\norigin 0x10\n1\n", false));
    assert!(a.inks == plain.inks);
    let checklist = std::fs::read_to_string(p("a.checklist.md")).unwrap();
    assert!(checklist.starts_with("# Assembler checklist: a") && checklist.contains("\n- a-1: \n") && checklist.contains("- Observed by: \n"));
    assert!(!checklist.contains("External Editing"));

    // assembly_external = <file>: External Editing on, the file beside the board, here with CRLF.
    std::fs::write(p("e.txt"), format!("assembly = field.vcbasm\nassembly_external = ext.vcbasm\nassembly_crlf = on\n{art}")).unwrap();
    assert!(vcb(&["gen", &p("e.txt"), "-o", &p("e.vcb")]).status.success());
    let e = Board::read(&d.join("e.vcb")).unwrap();
    assert!(e.assembly_is_external && e.assembly == a.assembly);
    assert_eq!(std::fs::read(p("e.vcbasm")).unwrap(), b"# case r-1\r\norigin 0x20\r\n2\r\n");
    let checklist = std::fs::read_to_string(p("e.checklist.md")).unwrap();
    assert!(checklist.contains("- a-1: \n- r-1: \n") && checklist.contains("External Editing switch"));
    assert!(!vcb(&["gen", &p("e.txt"), "-o", &p("e.vcb")]).status.success(), "an existing .vcbasm needs --force");

    // The program must be a file name next to the source.
    std::fs::write(p("bad.txt"), format!("assembly = ../field.vcbasm\n{art}")).unwrap();
    let out = vcb(&["gen", &p("bad.txt"), "-o", &p("bad.vcb")]);
    assert!(!out.status.success() && text(&out.stderr).contains("must be a file name in the same folder"));
}

// ---- the probe programs for the game (boards/asm-*.vcbasm, written by tools/board-generators/asm_probes.py) ----

#[test]
fn probe_programs_are_well_formed() {
    let read = |name: &str| std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("boards").join(name)).unwrap();
    for name in ["asm-probe-32.vcbasm", "asm-probe-16.vcbasm", "asm-reject.vcbasm", "asm-probe2.vcbasm"] {
        let program = read(name);
        // Case ids are unique, and so are the lines of the cases, so a commented-out line names one case.
        let ids: Vec<&str> = program.lines().filter_map(|l| l.strip_prefix("# case ")).collect();
        let lines: Vec<&str> = program.lines().filter(|l| !l.is_empty() && !l.starts_with("# case ")).collect();
        for list in [&ids, &lines] {
            let mut sorted = list.to_vec();
            sorted.sort();
            sorted.dedup();
            assert_eq!(sorted.len(), list.len(), "{name}: a case id or a line occurs twice");
        }
        assert!(ids.len() >= 30, "{name}");
    }

    // The probe holds no mistake of its own: with the flag, the engine's only objections are open questions
    // and what the game itself turned out to reject.
    let mut b = board(32);
    let objections = assemble(&read("asm-probe-32.vcbasm"), "probe", &mut b, true).unwrap_err() + "\n" + &assemble(&read("asm-probe2.vcbasm"), "probe2", &mut b, true).unwrap_err();
    for line in objections.lines() {
        assert!(line.contains("(facts question 23") || line.contains("no observed rule") || line.contains("the game rejects this"), "{line}");
    }
    // Every letter the probe boards are meant to answer has a case on one of them.
    let ids = read("asm-probe-32.vcbasm") + &read("asm-reject.vcbasm") + &read("asm-external.vcbasm") + &read("asm-novmem.vcbasm");
    for letter in 'a'..='x' {
        assert!(ids.contains(&format!("# case {letter}-")), "no probe case for 23{letter}");
    }
}

#[test]
fn macros_of_several_statements_match_the_game() {
    // The user's CPU board, assembled by the game on 2026-11-18: thirteen macros of several statements, with
    // variable and label names as arguments (one label is defined after its use).
    let game = Board::read(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/boards/asm-macros.assembled.vcb")).unwrap();
    // The board held three other words before; assembling again over the game's memory must change nothing.
    let mut ours = game.clone();
    let report = assemble(&game.assembly, "asm-macros", &mut ours, false).unwrap();
    assert_eq!((report.words.len(), report.replaced), (37, 0));
    assert!(ours.vmem == game.vmem, "the engine's words differ from the game's");
}
