//! `vcb gen` and `vcb run` from the command line (milestone M1).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn vcb(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vcb")).args(args).output().unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).unwrap()
}

/// Asserts the command failed with exactly one stderr line containing `needle`.
fn assert_fails(out: Output, needle: &str) {
    let stderr = text(&out.stderr);
    assert!(!out.status.success(), "expected a failure naming {needle:?}");
    assert_eq!(stderr.lines().count(), 1, "{stderr}");
    assert!(stderr.contains(needle), "{stderr:?} does not name {needle:?}");
}

/// Latch On -> buffer (on from tick 1) and Latch Off -> buffer (never on), probed at the buffers.
const TWO_BUFFERS: &str = "ticks = 0..2\nprobe.on = 4,0\nprobe.off = 4,2\n\nLW5RB\n\nlW5RB\n";

fn gen_two_buffers(d: &Path) -> String {
    let source = d.join("two.txt");
    std::fs::write(&source, TWO_BUFFERS).unwrap();
    let board = d.join("two.vcb");
    let out = vcb(&["gen", source.to_str().unwrap(), "-o", board.to_str().unwrap()]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    board.to_str().unwrap().to_string()
}

#[test]
fn gen_writes_board_checklist_and_picture() {
    let d = dir("gen");
    let board = gen_two_buffers(&d);
    let checklist = std::fs::read_to_string(d.join("two.checklist.md")).unwrap();
    assert!(checklist.contains("| 1 | on | 20,16 |"));
    assert!(checklist.contains("| 2 | off | 20,18 |"));
    for tick in 0..=2 {
        // An observation line ends right after the colon: no engine prediction.
        assert!(checklist.contains(&format!("- tick {tick}, lit LEDs: \n")));
    }
    assert!(d.join("two.png").exists());

    // Generating again is refused without --force, and a changed checklist is never overwritten.
    let source = d.join("two.txt");
    let again = |extra: &[&str]| vcb(&[&["gen", source.to_str().unwrap(), "-o", &board], extra].concat());
    assert_fails(again(&[]), "two.vcb: already exists");
    assert!(again(&["--force"]).status.success());
    std::fs::write(d.join("two.checklist.md"), checklist + "- tick 0, lit LEDs: 1\n").unwrap();
    assert_fails(again(&["--force"]), "two.checklist.md: already exists and differs");
    assert_fails(vcb(&["gen", source.to_str().unwrap(), "-o", source.to_str().unwrap()]), "is an input file");
}

#[test]
fn unverified_inks_are_refused_unless_allowed() {
    let d = dir("gate");
    // Verified inks run without a flag.
    let board = gen_two_buffers(&d);
    let out = vcb(&["run", &board, "--ticks", "2", "--probe", "20,16=on", "--probe", "20,18=off"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "tick 0: on=0 off=0\ntick 1: on=1 off=0\ntick 2: on=1 off=0\nstopped at tick 2: max-ticks\n");
    assert!(out.stderr.is_empty());

    let gen = |name: &str, art: &str| {
        let source = d.join(format!("{name}.txt"));
        std::fs::write(&source, art).unwrap();
        let board = d.join(format!("{name}.vcb"));
        assert!(vcb(&["gen", source.to_str().unwrap(), "-o", board.to_str().unwrap()]).status.success());
        board.to_str().unwrap().to_string()
    };

    // Timer is modelled: it needs --timer-ticks and warns that it is not game-exact.
    let timer = gen("timer", "\nIW5RE\n");
    let no_ticks = vcb(&["run", &timer, "--ticks", "2"]);
    assert!(!no_ticks.status.success() && text(&no_ticks.stderr).contains("--timer-ticks <n> is required"));
    let out = vcb(&["run", &timer, "--ticks", "2", "--timer-ticks", "5"]);
    assert!(out.status.success());
    assert_eq!(text(&out.stdout), "stopped at tick 2: max-ticks\n");
    assert_eq!(text(&out.stderr), "warning: Timer is modelled, not game-exact\n");

    // Everything is verified or modelled now, so the refusal is checked on a unit list of its own.
    let board = vcb::Board::read(Path::new(&gen("vmem", "vmem.address = 4,6,0,2,0\nvmem.content = 4,6,2,2,0\n\nl.l.l.l\n\nl.l.l.l\n"))).unwrap();
    assert!(vcb::unverified_units(&board).is_empty());
}

#[test]
fn run_argument_mistakes_are_named() {
    let d = dir("args");
    let board = gen_two_buffers(&d);
    let run = |extra: &[&str]| vcb(&[&["run", &board, "--ticks", "2", "--allow-unverified"], extra].concat());
    let last_line = |out: Output| {
        let stderr = text(&out.stderr);
        assert!(!out.status.success());
        stderr.lines().last().unwrap().to_string()
    };
    assert!(last_line(run(&["--probe", "0,0"])).contains("no entity at 0,0"));
    assert!(last_line(run(&["--probe", "20,16=a", "--probe", "20,18=a"])).contains("label \"a\" is used twice"));
    assert!(last_line(run(&["--render", "3:x.png"])).contains("tick 3 is after the run's last tick 2"));
    assert!(last_line(run(&["--render", &format!("1:{board}")])).contains("is an input file"));
    let png = d.join("two.png"); // written by gen
    assert!(last_line(run(&["--render", &format!("1:{}", png.display())])).contains("already exists"));
    assert!(last_line(run(&["--clock-interval", "0"])).contains("clock_interval"));
    assert!(!vcb(&["run", &board]).status.success(), "--ticks is required");
}

#[test]
fn render_keeps_the_colour_when_on_and_halves_it_when_off() {
    let d = dir("render");
    let board = gen_two_buffers(&d);
    let png = d.join("t2.png");
    let out = vcb(&["run", &board, "--ticks", "2", "--allow-unverified", "--render", &format!("2:{}", png.display())]);
    assert!(out.status.success(), "{}", text(&out.stderr));

    let mut reader = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&png).unwrap())).read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut pixels).unwrap();
    // The art is 5x3 board pixels, drawn at 8 image pixels each.
    assert_eq!((info.width, info.height), (40, 24));
    let at = |x: usize, y: usize| &pixels[(y * 40 + x) * 4..][..4];
    assert_eq!(at(4 * 8 + 3, 3), [0x92, 0xff, 0x63, 0xff]); // the buffer that is on
    assert_eq!(at(4 * 8 + 3, 2 * 8 + 3), [0x49, 0x7f, 0x31, 0xff]); // the buffer that is off
    assert_eq!(at(4 * 8 + 3, 8 + 3), [0, 0, 0, 0]); // empty
}

#[test]
fn watch_and_dump_show_the_memory_after_the_run() {
    let d = dir("vmem");
    let board = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/boards/vmem-preserve.vcb");
    let board = board.to_str().unwrap();
    let dump = d.join("dump.txt");
    let run = |extra: &[&str]| vcb(&[&["run", board, "--ticks", "100", "--allow-unverified"], extra].concat());

    let out = run(&["--watch", "1,0x2", "--dump-vmem", dump.to_str().unwrap()]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(text(&out.stdout).ends_with("0x00000001 = 0x00000003 (3)\n0x00000002 = 0x00000000 (0)\n"), "{}", text(&out.stdout));
    let lines: Vec<String> = std::fs::read_to_string(&dump).unwrap().lines().map(String::from).collect();
    assert_eq!(lines.len(), 1 << 20);
    assert_eq!(&lines[..3], ["0x00000000 0x00000000", "0x00000001 0x00000003", "0x00000002 0x00000000"]);
    assert_eq!(lines[(1 << 20) - 1], "0x000FFFFF 0x00000000");

    let error = |out: Output| text(&out.stderr).lines().last().unwrap().to_string();
    assert!(error(run(&["--dump-vmem", dump.to_str().unwrap()])).contains("already exists"));
    assert!(run(&["--dump-vmem", dump.to_str().unwrap(), "--force"]).status.success());
    assert!(error(run(&["--dump-vmem", board, "--force"])).contains("is an input file"));
    assert!(error(run(&["--watch", "0x100000"])).contains("not a VMem address"));

    // A board without VMem has nothing to watch or dump.
    let plain = gen_two_buffers(&d);
    assert_fails(vcb(&["run", &plain, "--ticks", "1", "--watch", "1"]), "is_vmem_enabled");
}
