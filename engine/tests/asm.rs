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

/// Each unverified feature: a gate hit naming it without the flag; with the flag, the manual's word.
#[test]
fn unverified_features_need_the_flag_and_follow_the_manual() {
    let o = "origin 0x10\n";
    let cases: &[(&str, &str, &[(usize, u32)])] = &[
        ("first address is 1", "5", &[(1, 5)]),
        ("negative literal", "origin 0x10\n-1", &[(0x10, 0xFFFF_FFFF)]),
        ("underscores in decimal and hex literals", "origin 0x10\n1_000; 0x1_0", &[(0x10, 1000), (0x11, 16)]),
        ("comment after a statement", "origin 0x10\n3 # c", &[(0x10, 3)]),
        ("OR of overlapping bits", "origin 0x10\nsymbol a 3; symbol b 6; a b", &[(0x10, 7)]),
        ("more than two primitives in an instruction", "origin 0x10\nsymbol a 1; a 2 4", &[(0x10, 7)]),
        ("resymb", "origin 0x10\nsymbol a 1; resymb a 2; a", &[(0x10, 2)]),
        ("unsymb", "origin 0x10\nsymbol a 1; unsymb a; 5", &[(0x10, 5)]),
        ("pointer inline", "origin 0x10\n1; pointer p inline 5; 2; p", &[(0x10, 1), (0x11, 5), (0x12, 2), (0x13, 0x11)]),
        ("unpoint", "pointer p 0x20 1; unpoint p\norigin 0x10\n5", &[(0x10, 5), (0x20, 1)]),
        ("origin orgprev", "origin 0x10\norigin 0x20; origin orgprev; 7", &[(0x10, 7)]),
        ("origin orgbase", "origin 0x10\norigin 0x20; origin 0x30; origin orgbase; 7", &[(1, 7)]),
        ("operator +", "origin 0x10\n6 + 3", &[(0x10, 9)]),
        ("operator -", "origin 0x10\n6 - 3", &[(0x10, 3)]),
        ("operator *", "origin 0x10\n6 * 3", &[(0x10, 18)]),
        ("operator &", "origin 0x10\n6 & 3", &[(0x10, 2)]),
        ("operator ^", "origin 0x10\n6 ^ 3", &[(0x10, 5)]),
        ("operator <<", "origin 0x10\n1 << 4", &[(0x10, 16)]),
        ("operator /", "origin 0x10\n6 / 3", &[(0x10, 2)]),
        ("operator %", "origin 0x10\n7 % 4", &[(0x10, 3)]),
        ("an expression outside the verified macro shape", "origin 0x10\n6 | 1; (8 >> 1)", &[(0x10, 7), (0x11, 4)]),
        ("macro without exactly two placeholders", "origin 0x10\nsymbol s 5; macro m { a? }; m s", &[(0x10, 5)]),
        ("macro without exactly two placeholders", "origin 0x10\nsymbol x 1; symbol y 2; symbol z 4\nmacro m { a? | b? | c? }; m x y z", &[(0x10, 7)]),
        ("macro argument that is a literal", "origin 0x10\nmacro m { a? | b? }; m 1 2", &[(0x10, 3)]),
        ("macro braces without spaces", "origin 0x10\nsymbol x 1; symbol y 2\nmacro m {a? | b?}; m x y", &[(0x10, 3)]),
        ("remac", "origin 0x10\nsymbol x 1; symbol y 2\nmacro m { a? | b? }; remac m { a? | b? | x }; m y y", &[(0x10, 3)]),
        ("unmac", "origin 0x10\nmacro m { a? | b? }; unmac m; 5", &[(0x10, 5)]),
        ("bookmark", "origin 0x10\n1; bookmark X; 2", &[(0x10, 1), (0x11, 2)]),
        ("sub_bookmark", "origin 0x10\n1; sub_bookmark Y; 2", &[(0x10, 1), (0x11, 2)]),
    ];
    let _ = o;
    for (feature, source, words) in cases {
        let refused = asm(source, false).expect_err(feature);
        assert!(refused.contains(&format!("'{feature}' is not verified")), "{feature}: {refused}");
        assert_eq!(&asm(source, true).unwrap_or_else(|e| panic!("{feature}: {e}")), words, "{feature}");
    }

    // With the flag a feature is reported once, however often it is used.
    let mut b = board(32);
    let report = assemble("origin 0x10\n6 + 3; 1 + 1", "t", &mut b, true).unwrap();
    assert_eq!(report.unverified, ["an expression outside the verified macro shape", "operator +"]);
    assert!(report.summary("t").ends_with("(unverified: asm an expression outside the verified macro shape, asm operator +)"));
}

/// Every lettered case of facts question 23 is a gate hit naming its letter, with and without the flag.
#[test]
fn undefined_cases_are_always_gate_hits() {
    let cases: &[(&str, &str)] = &[
        ("a", "origin 0x10\n1 | 2 << 3"),
        ("b", "origin 0x10\n0x1_0000_0000"),
        ("b", "origin 0x10\n1 - 2"),
        ("b", "origin 0x10\n0xFFFFFFFF + 1"),
        ("c", "origin 0x10\n6 / 0"),
        ("c", "origin 0x10\n6 % 0"),
        ("c", "origin 0x10\n1 << 32"),
        ("d", "symbol a 1; symbol a 2"),
        ("d", "pointer p 0x20 1; pointer p 0x21 1"),
        ("d", "macro m { a? | b? }; macro m { a? | b? }"),
        ("d", "origin 0x10\n@ L\n1\n@ L\n2"),
        ("e", "symbol a 1; pointer a 0x20 1"),
        ("e", "symbol a 1\norigin 0x10\n@ a\n1"),
        ("e", "symbol origin 1"),
        ("f", "origin 0x10\nnope"),
        ("f", "origin 0x10\nsymbol a 1; unsymb a; a"),
        ("f", "pointer p 0x20 1; unpoint p\norigin 0x10\np"),
        ("g", "origin 0x10\na; symbol a 1"),
        ("g", "origin 0x10\np; pointer p 0x20 1"),
        ("h", "origin 0x10\nsymbol x 1; macro m { a? | b? }; m x"),
        ("h", "origin 0x10\nsymbol x 1; macro m { a? | b? }; m x x x"),
        ("i", "origin 0x10\nsymbol x 1; macro m { a? | b? }; m ( x"),
        ("j", "origin 0x10\nsymbol x 1; macro m { a? | b? }; x m"),
        ("k", "origin 0x10\nmacro m {\na? | b?\n}"),
        ("k", "origin 0x10\nsymbol x 1; macro n { a? | b? }; macro m { n | a? | b? }; m x x"),
        ("l", "pointer p 0x20 1; 5"),
        ("l", "origin 0x10\npointer p 0x20 1; 5"),
        ("m", "origin 0x10\n1\norigin 0x10\n2"),
        ("m", "pointer p 0x10 1\norigin 0x10\n2"),
        ("n", "origin 0x10000"),
        ("n", "origin 0"),
        ("n", "pointer p 0 1"),
        ("n", "origin 0xFFFF\n1; 2"),
        ("o", "origin orgprev"),
        ("o", "origin orgbase"),
        ("p", "symbol a 1; symbol b a"),
        ("p", "symbol a 1; origin a"),
        ("p", "origin 0x10\n1|2"),
        ("q", "origin 0x10\n@ L 5"),
        ("q", "Symbol a 1"),
        ("q", "origin 0X10"),
        ("q", "origin 0xff"),
        ("q", "origin 0x10\n+5"),
        ("r", "origin 0x10\r\n5"),
        ("s", "origin 0x10\n@ L\norigin 0x20\n1"),
        ("s", "origin 0x10\n@ L\npointer p 0x20 1"),
        ("s", "origin 0x10\n1\n@ L"),
        ("t", "# a; b"),
        ("u", "origin 0x10\n~ 1"),
        ("u", "origin 0x10\n7 / 2"),
        ("v", "pointer p 0x20 1; repoint p 0x21 1"),
        ("w", "origin 0x10\n8 >> 1 >> 1"),
        ("w", "origin 0x10\n8 - 1 - 1"),
        ("x", "symbol a.b 1"),
        ("x", "origin 0x10\nsymbol 1a 1"),
    ];
    for letter in 'a'..='x' {
        assert!(cases.iter().any(|(l, _)| *l == letter.to_string()), "no case for 23{letter}");
    }
    for (letter, source) in cases {
        for allow in [false, true] {
            let refused = asm(source, allow).expect_err(source);
            assert!(refused.contains(&format!("(facts question 23{letter})")), "23{letter} {source:?}: {refused}");
        }
    }
    // A negative literal on a narrow board is 23b, whatever the flag.
    let mut narrow = board(16);
    assert!(assemble("origin 0x10\n-1", "t", &mut narrow, true).unwrap_err().contains("23b"));
    assert!(assemble("origin 0x10\n0x10000", "t", &mut narrow, true).unwrap_err().contains("23b"));
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
    // Three gated statements and no real error: all three, and the board is unchanged.
    let mut b = board(32);
    let error = assemble("origin 0x10\n6 + 3\n-1\nnope", "t", &mut b, false).unwrap_err();
    let lines: Vec<&str> = error.lines().collect();
    assert_eq!(lines.len(), 4, "{error}"); // `6 + 3` is two features
    assert!(lines[0].starts_with("t:2:1: ") && lines[2].starts_with("t:3:1: ") && lines[3].starts_with("t:4:1: "), "{error}");
    assert!(b.vmem.iter().all(|w| *w == 0));

    // A gate hit, then a real error: both, the gate hit first; nothing after the real error.
    let error = asm("origin 0x10\n-1\n(1\nnope", false).unwrap_err();
    let lines: Vec<&str> = error.lines().collect();
    assert_eq!(lines.len(), 2, "{error}");
    assert!(lines[0].contains("negative literal") && lines[1].starts_with("t:3:1: "), "{error}");

    // Positions: after a `;`, and inside a macro use.
    assert!(asm("origin 0x10\n1; foo", false).unwrap_err().starts_with("t:2:4: "));
    let error = asm("symbol x 1\nmacro m { a? | b? | nope }\norigin 0x10\nm x x", false).unwrap_err();
    assert!(error.contains("t:4:1: ") && error.contains("(in macro m)"), "{error}");
}

#[test]
fn empty_sources_and_word_0() {
    for source in ["", " \n\t\n", "symbol a 1", "# only a comment\n"] {
        let mut b = board(32);
        let report = assemble(source, "t", &mut b, false).unwrap();
        assert!(report.words.is_empty() && b.vmem.iter().all(|w| *w == 0), "{source:?}");
    }
    assert_eq!(assemble("symbol a 1", "t", &mut board(32), false).unwrap().summary("t"), "assembled 0 words from t");

    // On a 16-bit board the upper bits of a stored word are zero.
    let mut narrow = board(16);
    narrow.vmem[0x10] = 0xABCD_0000;
    assemble("origin 0x10\n0xFFFF", "t", &mut narrow, false).unwrap();
    assert_eq!((narrow.vmem[0], narrow.vmem[0x10]), (0, 0xFFFF));
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
    std::fs::write(p("bad.vcbasm"), "origin 0x10\n-1\n(1\n").unwrap();
    let out = vcb(&["asm", &b, "--asm", &p("bad.vcbasm"), "-o", &p("never.vcb"), "--listing", &p("never.txt")]);
    let stderr = text(&out.stderr);
    assert!(!out.status.success() && stderr.contains("bad.vcbasm:2:1: 'negative literal'") && stderr.contains("bad.vcbasm:3:1: "), "{stderr}");
    assert!(!d.join("never.vcb").exists() && !d.join("never.txt").exists() && !d.join("never.vcb.part").exists());

    std::fs::write(p("latin1.vcbasm"), b"origin 0x10\n1 # \xff\n").unwrap();
    fails(&["asm", &b, "--asm", &p("latin1.vcbasm")], "latin1.vcbasm: is not valid UTF-8");
}

#[test]
fn vcb_asm_external_editing_is_gated() {
    let d = dir("asm-external");
    let mut b = board(32);
    b.assembly = "origin 0x10\n1".to_string();
    b.assembly_is_external = true;
    let path = d.join("x.vcb");
    std::fs::write(&path, b.to_vcb()).unwrap();
    let (board_path, listing) = (path.to_str().unwrap(), d.join("l.txt"));
    let run = |allow: bool| {
        let mut args = vec!["asm", board_path, "--listing", listing.to_str().unwrap(), "--force"];
        args.extend(allow.then_some("--allow-unverified"));
        let out = vcb(&args);
        (out.status.success(), text(&out.stdout) + &text(&out.stderr))
    };
    let (ok, said) = run(false);
    assert!(!ok && said.contains("'External Editing source' is not verified"), "{said}");
    let (ok, said) = run(true);
    assert!(!ok && said.contains("x.vcbasm") && said.contains("23r"), "{said}");

    // With the file there, it is assembled and the board's own text is not.
    std::fs::write(d.join("x.vcbasm"), "origin 0x10\r\n2\r\n").unwrap();
    let (ok, said) = run(true);
    assert!(ok && said.contains("(unverified: asm External Editing source)") && said.contains("warning: asm External Editing source"), "{said}");
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

    // A gate hit stops the run before tick 0; with the flag the feature is named on both lines.
    std::fs::write(p("neg.vcbasm"), "origin 0x10\n-1\n").unwrap();
    let (ok, stdout, stderr) = run(&["--asm", &p("neg.vcbasm")]);
    assert!(!ok && stdout.is_empty() && stderr.contains("neg.vcbasm:2:1: 'negative literal' is not verified"), "{stderr}");
    let (ok, stdout, stderr) = run(&["--asm", &p("neg.vcbasm"), "--allow-unverified"]);
    assert!(ok && stderr.contains("warning: asm negative literal is not verified"), "{stderr}");
    assert!(stdout.contains("(unverified: asm negative literal)\nstopped at tick 1: max-ticks (unverified: asm negative literal)\n"), "{stdout}");

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
