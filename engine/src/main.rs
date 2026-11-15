use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;
use vcb::format::Result;
use vcb::netlist::Components;
use vcb::sim::{Engine, Simulation, StopSet};
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
    Info {
        board: PathBuf,
        /// List how often each pair of inks touches, for pairs involving Read, Write, Cross, Tunnel, Mesh or Bus.
        #[arg(long)]
        adjacency: bool,
    },
    /// Turn a text-art board into a .vcb, a blank checklist and a picture with numbered probes.
    Gen {
        source: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        /// Allow overwriting the .vcb and the picture. A filled-in checklist is never overwritten.
        #[arg(long)]
        force: bool,
    },
    /// Simulate a board.
    Run(RunArgs),
}

#[derive(clap::Args)]
struct RunArgs {
    board: PathBuf,
    /// Maximum number of ticks to run.
    #[arg(long)]
    ticks: u64,
    /// Print the on/off state of the entity at this pixel every tick: x,y or x,y=label. Repeatable.
    #[arg(long)]
    probe: Vec<String>,
    /// Write a PNG snapshot after a tick: tick:file.png. Repeatable.
    #[arg(long)]
    render: Vec<String>,
    /// Ignore Breakpoint inks.
    #[arg(long)]
    no_breakpoint: bool,
    /// Run even if the board uses inks that have no observed fixture yet.
    #[arg(long)]
    allow_unverified: bool,
    #[arg(long)]
    clock_interval: Option<i64>,
    /// Print ticks, wall time and ticks per second.
    #[arg(long)]
    stats: bool,
    /// Allow overwriting existing output files.
    #[arg(long)]
    force: bool,
}

fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Command::Info { board, adjacency: false } => Board::read(&board).map(|b| print!("{}", vcb::info(&b))),
        Command::Info { board, adjacency: true } => Board::read(&board).map(|b| {
            for ((a, b), n) in vcb::adjacency(&b) {
                println!("{a} + {b}: {n}");
            }
        }),
        Command::Gen { source, output, force } => gen(&source, &output, force),
        Command::Run(args) => run(args),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Fails if `output` is one of the files the command reads, or is listed twice.
fn check_outputs(inputs: &[&Path], outputs: &[&Path]) -> Result<()> {
    let real = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    for (i, out) in outputs.iter().enumerate() {
        if inputs.iter().any(|input| real(input) == real(out)) {
            return Err(format!("{}: is an input file of this command", out.display()));
        }
        if outputs[..i].iter().any(|other| real(other) == real(out)) {
            return Err(format!("{}: is given as an output twice", out.display()));
        }
    }
    Ok(())
}

fn check_overwrite(path: &Path, force: bool) -> Result<()> {
    if path.exists() && !force {
        return Err(format!("{}: already exists (use --force to overwrite)", path.display()));
    }
    Ok(())
}

/// Writes through a temporary file so a failed write never leaves half a file.
fn write_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".part");
    std::fs::write(&tmp, bytes).and_then(|()| std::fs::rename(&tmp, path)).map_err(|e| format!("{}: {e}", path.display()))
}

fn gen(source: &Path, output: &Path, force: bool) -> Result<()> {
    let text = std::fs::read_to_string(source).map_err(|e| format!("{}: {e}", source.display()))?;
    let tb = vcb::textboard::parse(&text).map_err(|e| format!("{}: {e}", source.display()))?;
    let name = output.file_stem().and_then(|s| s.to_str()).ok_or("the output needs a file name")?;
    let (checklist_path, png_path) = (output.with_extension("checklist.md"), output.with_extension("png"));

    let vcb_text = tb.board.to_vcb();
    let checklist = vcb::checklist(name, &vcb_text, &tb);
    let labels: Vec<_> = tb.probes.iter().enumerate().map(|(i, p)| (i + 1, p.x, p.y)).collect();
    let png = vcb::render::render(&tb.board, None, &labels)?.to_png();

    // Check everything before writing anything.
    check_outputs(&[source], &[output, &checklist_path, &png_path])?;
    check_overwrite(output, force)?;
    check_overwrite(&png_path, force)?;
    if let Ok(existing) = std::fs::read_to_string(&checklist_path) {
        if existing != checklist {
            return Err(format!("{}: already exists and differs; it may hold observations, so it is never overwritten", checklist_path.display()));
        }
    }
    write_file(output, vcb_text.as_bytes())?;
    write_file(&checklist_path, checklist.as_bytes())?;
    write_file(&png_path, &png)?;
    println!("wrote {}, {} and {}", output.display(), checklist_path.display(), png_path.display());
    Ok(())
}

fn run(args: RunArgs) -> Result<()> {
    let mut board = Board::read(&args.board)?;
    if let Some(n) = args.clock_interval {
        board.settings.clock_interval = n;
    }

    // The gate: refuse inks nobody has observed in the game yet.
    let unverified = vcb::unverified_units(&board);
    if let Some((unit, at)) = unverified.first().filter(|_| !args.allow_unverified) {
        let place = at.map(|(x, y)| format!(" at {x},{y}")).unwrap_or_default();
        return Err(format!("{unit}{place} is not verified yet (use --allow-unverified to run anyway)"));
    }
    for (unit, _) in &unverified {
        eprintln!("warning: {unit} is not verified yet");
    }

    let components = Components::build(&board)?;
    let mut probes: Vec<(String, u32)> = Vec::new();
    for spec in &args.probe {
        let (place, label) = spec.split_once('=').unwrap_or((spec, spec));
        let xy = place.split_once(',').and_then(|(x, y)| Some((x.trim().parse().ok()?, y.trim().parse().ok()?)));
        let (x, y): (usize, usize) = xy.ok_or(format!("--probe {spec}: expected x,y or x,y=label"))?;
        let entity = components.entity_at(x, y).ok_or(format!("--probe {spec}: no entity at {x},{y}"))?;
        if probes.iter().any(|(l, _)| l == label) {
            return Err(format!("--probe {spec}: label \"{label}\" is used twice"));
        }
        probes.push((label.to_string(), entity));
    }

    let mut renders: Vec<(u64, PathBuf)> = Vec::new();
    for spec in &args.render {
        let (tick, file) = spec.split_once(':').ok_or(format!("--render {spec}: expected tick:file.png"))?;
        let tick: u64 = tick.parse().map_err(|_| format!("--render {spec}: \"{tick}\" is not a tick number"))?;
        if tick > args.ticks {
            return Err(format!("--render {spec}: tick {tick} is after the run's last tick {}", args.ticks));
        }
        renders.push((tick, file.into()));
    }
    let render_paths: Vec<&Path> = renders.iter().map(|(_, p)| p.as_path()).collect();
    check_outputs(&[&args.board, &args.board.with_extension("vcbmem")], &render_paths)?;
    for path in &render_paths {
        check_overwrite(path, args.force)?;
    }

    let mut engine = Engine::new(&board, components)?;
    let mut render_error = None;
    let started = Instant::now();
    let stops = StopSet { max_ticks: args.ticks, breakpoint: !args.no_breakpoint };
    let report = engine.run_until(&stops, |engine| {
        if !probes.is_empty() {
            let values: Vec<String> = probes.iter().map(|(label, e)| format!("{label}={}", engine.state()[*e as usize] as u8)).collect();
            println!("tick {}: {}", engine.tick(), values.join(" "));
        }
        for (_, path) in renders.iter().filter(|(tick, _)| *tick == engine.tick()) {
            let written = vcb::render::render(&board, Some((&engine.components, engine.state())), &[]).and_then(|image| write_file(path, &image.to_png()));
            render_error = render_error.take().or(written.err());
        }
    });
    let elapsed = started.elapsed().as_secs_f64();
    if let Some(e) = render_error {
        return Err(e);
    }

    let mut line = format!("stopped at tick {}: {}", report.tick, report.reasons.join(", "));
    if !unverified.is_empty() {
        let units: Vec<&str> = unverified.iter().map(|(unit, _)| *unit).collect();
        line += &format!(" (unverified: {})", units.join(", "));
    }
    println!("{line}");
    if let Some((tick, path)) = renders.iter().find(|(tick, _)| *tick > report.tick) {
        return Err(format!("--render {tick}:{}: the run stopped at tick {} before that tick", path.display(), report.tick));
    }
    if args.stats {
        eprintln!("ticks: {}  time: {elapsed:.3} s  TPS: {:.0}", report.tick, report.tick as f64 / elapsed.max(1e-9));
    }
    Ok(())
}
