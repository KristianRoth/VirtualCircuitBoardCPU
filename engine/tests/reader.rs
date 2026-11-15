//! Reader, writer and `vcb info` tests (milestone M0).

use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use vcb::{Board, Ink};

fn repo(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(path)
}

fn tmp(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A small valid board with a few inks and VMem words.
fn sample() -> Board {
    let mut b = Board::new(8, 4);
    b.set(1, 1, Ink::Trace(7));
    b.set(2, 1, Ink::Bus(3));
    b.set(3, 2, Ink::Xnor);
    b.vmem[1] = 0xdead_beef;
    b.vmem[(1 << 20) - 1] = 7;
    b.settings.clock_interval = 3;
    b
}

fn no_vcbmem() -> vcb::format::Result<Vec<u8>> {
    panic!("external VMem was not expected")
}

/// The sample board's JSON after `edit`, parsed again.
fn parse_edited(edit: impl FnOnce(&mut Value)) -> vcb::format::Result<Board> {
    let mut json: Value = serde_json::from_str(&sample().to_vcb()).unwrap();
    edit(&mut json);
    Board::parse(&json.to_string(), no_vcbmem)
}

fn blob(raw: &[u8], trailer: &[i32]) -> String {
    let mut bytes = zstd::bulk::compress(raw, 3).unwrap();
    for v in trailer {
        bytes.extend(v.to_le_bytes());
    }
    STANDARD.encode(bytes)
}

fn assert_error(result: vcb::format::Result<Board>, needle: &str) {
    let error = result.expect_err("expected a reader error");
    assert!(error.contains(needle), "error {error:?} does not name {needle:?}");
    assert!(!error.contains('\n'), "error {error:?} is more than one line");
}

#[test]
fn written_board_reads_back_equal() {
    let board = sample();
    assert!(Board::parse(&board.to_vcb(), no_vcbmem).unwrap() == board);
}

#[test]
fn game_saves_survive_a_rewrite() {
    for file in ["saves/all-inks.vcb", "32-bit-alu-try.vcb"] {
        let board = Board::read(&repo(file)).unwrap();
        let again = Board::parse(&board.to_vcb(), no_vcbmem).unwrap();
        assert!(again == board, "{file} changed when re-written");
    }
}

#[test]
fn all_inks_save_has_one_pixel_of_each_ink() {
    let board = Board::read(&repo("saves/all-inks.vcb")).unwrap();
    for (_, ink) in vcb::ink::COLOURS {
        assert_eq!(board.inks.iter().filter(|i| **i == ink).count(), 1, "{ink}");
    }
}

#[test]
fn info_matches_expected_output() {
    for (file, expected) in [("32-bit-alu-try.vcb", "info-cpu.txt"), ("saves/all-inks.vcb", "info-all-inks.txt")] {
        let board = Board::read(&repo(file)).unwrap();
        let expected = std::fs::read_to_string(repo("engine/tests/expected").join(expected)).unwrap();
        assert_eq!(vcb::info(&board), expected, "{file}");
    }
}

#[test]
fn old_tres_format_is_rejected() {
    assert_error(Board::parse("[gd_resource type=\"Resource\"]\n", no_vcbmem), "[gd_resource");
}

#[test]
fn bad_header_fields_are_named() {
    assert_error(parse_edited(|j| j["file_type"] = "OTHER".into()), "file_type");
    assert_error(parse_edited(|j| j["file_version"] = 3.into()), "file_version");
    assert_error(parse_edited(|j| j["vinput_is_enabled"] = true.into()), "vinput_is_enabled");
    assert_error(parse_edited(|j| j["vdisplay_is_enabled"] = true.into()), "vdisplay_is_enabled");
}

#[test]
fn missing_fields_are_named() {
    for name in ["file_type", "file_version", "layers", "vmem_data", "vmem_settings", "is_vmem_enabled", "vmem_is_external", "clock_interval", "timer_interval", "random_seed", "random_is_time_seed", "vinput_is_enabled", "vdisplay_is_enabled"] {
        assert_error(parse_edited(|j| { j.as_object_mut().unwrap().remove(name); }), name);
    }
}

#[test]
fn bad_blobs_are_named() {
    assert_error(parse_edited(|j| j["layers"][0] = "not base64!".into()), "layers[0]: bad base64");
    assert_error(parse_edited(|j| j["vmem_data"] = "!!".into()), "vmem_data: bad base64");
    let junk = STANDARD.encode([&[1u8, 2, 3, 4][..], &2i32.to_le_bytes(), &16i32.to_le_bytes()].concat());
    assert_error(parse_edited(|j| j["vmem_data"] = junk.into()), "vmem_data: bad zstd");
    // Trailer size differs from the decompressed size.
    assert_error(parse_edited(|j| j["layers"][0] = blob(&[0; 128], &[2, 4, 2, 8, 2, 100]).into()), "layers[0]");
    // Width x height differs from the size.
    assert_error(parse_edited(|j| j["layers"][0] = blob(&[0; 128], &[2, 4, 2, 9, 2, 128]).into()), "layers[0]: 9x4");
    // A size over the cap is refused before anything is allocated.
    assert_error(parse_edited(|j| j["layers"][0] = blob(&[0; 128], &[2, 40000, 2, 40000, 2, i32::MAX]).into()), "64 MiB");
    assert_error(parse_edited(|j| j["layers"][0] = blob(&[0; 128], &[2, 4, 3, 8, 2, 128]).into()), "bad trailer");
    assert_error(parse_edited(|j| j["vmem_data"] = blob(&[0; 64], &[2, 64]).into()), "vmem_data: is 64 bytes");
}

#[test]
fn unknown_colour_names_the_pixel() {
    let mut pixels = vec![0u8; 8 * 4 * 4];
    pixels[(2 * 8 + 5) * 4..][..4].copy_from_slice(&[0x12, 0x34, 0x56, 0xff]);
    assert_error(parse_edited(|j| j["layers"][0] = blob(&pixels, &[2, 4, 2, 8, 2, 128]).into()), "5,2 #123456ff");
}

#[test]
fn vmem_with_one_extra_byte_is_accepted() {
    let board = parse_edited(|j| j["vmem_data"] = blob(&vec![0; (4 << 20) + 1], &[2, (4 << 20) + 1]).into()).unwrap();
    assert_eq!(board.vmem.len(), 1 << 20);
}

#[test]
fn word_zero_is_cleared_on_load() {
    let mut board = sample();
    board.vmem[0] = 5;
    assert_eq!(Board::parse(&board.to_vcb(), no_vcbmem).unwrap().vmem[0], 0);
}

#[test]
fn external_vmem_is_read_from_the_vcbmem_file() {
    let dir = tmp("external");
    let mut json: Value = serde_json::from_str(&sample().to_vcb()).unwrap();
    json["vmem_is_external"] = true.into();
    json["vmem_data"] = STANDARD.encode([2i32.to_le_bytes(), 0i32.to_le_bytes()].concat()).into();
    let vcb = dir.join("board.vcb");
    let mem = dir.join("board.vcbmem");
    std::fs::write(&vcb, json.to_string()).unwrap();

    let _ = std::fs::remove_file(&mem);
    assert_error(Board::read(&vcb), "board.vcbmem");

    std::fs::write(&mem, [0u8; 100]).unwrap();
    assert_error(Board::read(&vcb), "board.vcbmem: is 100 bytes");

    // Word i = 0x1122330i, as in the pattern verified in the game.
    let bytes: Vec<u8> = (0..1u32 << 20).flat_map(|i| (0x1122_3300 | (i & 0xf)).to_be_bytes()).collect();
    std::fs::write(&mem, bytes).unwrap();
    let board = Board::read(&vcb).unwrap();
    assert_eq!(board.vmem[0], 0);
    assert_eq!(board.vmem[1], 0x1122_3301);
    assert_eq!(board.vmem[(1 << 20) - 1], 0x1122_330f);
}

#[test]
fn cli_reports_a_reader_error_on_one_line() {
    let file = tmp("cli").join("old.vcb");
    std::fs::write(&file, "[gd_resource type=\"Resource\"]\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_vcb")).arg("info").arg(&file).output().unwrap();
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert_eq!(stderr.lines().count(), 1, "{stderr}");
    assert!(stderr.contains("[gd_resource"));
}

#[test]
fn cli_info_prints_the_board() {
    let out = Command::new(env!("CARGO_BIN_EXE_vcb")).arg("info").arg(repo("saves/all-inks.vcb")).output().unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8(out.stdout).unwrap().contains("inks: 48"));
}
