use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;
use vcb::Board;

#[derive(Parser)]
#[command(name = "vcb", about = "Simulation engine for Virtual Circuit Board .vcb files")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the size, settings and ink counts of a board.
    Info { board: PathBuf },
}

fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Command::Info { board } => Board::read(&board).map(|b| print!("{}", vcb::info(&b))),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
