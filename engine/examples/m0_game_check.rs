//! Writes the three boards for the M0 "opens in the game" check into saves/pending/.
//! Run from engine/: cargo run --example m0_game_check

use std::path::Path;
use vcb::{Board, Ink};

fn main() -> Result<(), String> {
    let out = Path::new("../saves/pending");
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;

    // 1. A board built from scratch: Latch On -> Write -> trace -> Read -> Buffer, at 1000,1000.
    let mut board = Board::new(2048, 2048);
    let column = [Ink::LatchOn, Ink::Write, Ink::Trace(5), Ink::Trace(5), Ink::Read, Ink::Buffer];
    for (i, ink) in column.into_iter().enumerate() {
        board.set(1000, 1000 + i, ink);
    }
    write(&out.join("m0-new.vcb"), &board)?;

    // 2 and 3. Game saves read and written again.
    write(&out.join("m0-all-inks.vcb"), &Board::read(Path::new("../saves/all-inks.vcb"))?)?;
    write(&out.join("m0-cpu.vcb"), &Board::read(Path::new("../32-bit-alu-try.vcb"))?)
}

fn write(path: &Path, board: &Board) -> Result<(), String> {
    std::fs::write(path, board.to_vcb()).map_err(|e| format!("{}: {e}", path.display()))?;
    println!("wrote {}", path.display());
    Ok(())
}
