use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;
use vcb::format::Result;
use vcb::netlist::Components;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use vcb::sim::{Access, Engine, Options, Simulation, StopSet};
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
    /// Assemble a program into a board's memory, as the game does when a simulation starts.
    Asm(AsmArgs),
}

#[derive(clap::Args)]
struct AsmArgs {
    board: PathBuf,
    /// The program. Default: the board's own (its Assembly Editor text, or <board>.vcbasm with External Editing).
    #[arg(long)]
    asm: Option<PathBuf>,
    /// Write the board with the assembled memory, and the program as its Assembly Editor text.
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// Write all of VMem after assembling, one `0xADDRESS 0xVALUE` line per word.
    #[arg(long)]
    dump_vmem: Option<PathBuf>,
    /// Write address, word, line and statement of every stored word, then the labels and pointers. "-" prints it.
    #[arg(long)]
    listing: Option<PathBuf>,
    /// Use language features that the manual states but no game save shows yet.
    #[arg(long)]
    allow_unverified: bool,
    /// Allow overwriting existing output files.
    #[arg(long)]
    force: bool,
}

#[derive(clap::Args)]
struct RunArgs {
    board: PathBuf,
    /// A TOML file with run settings. Keys are the long flag names (ticks, break-addr, watch, ...);
    /// flags given on the command line win.
    #[arg(long)]
    config: Option<PathBuf>,
    /// Maximum number of ticks to run. Required, here or in the config file.
    #[arg(long)]
    ticks: Option<u64>,
    /// Stop when VMem accesses this address: addr[:fetch|write|any] (default fetch). Repeatable.
    #[arg(long)]
    break_addr: Vec<String>,
    /// Stop once VMem has not changed for this many ticks.
    #[arg(long)]
    quiescent: Option<u64>,
    /// Which engine to run. Only v1 exists.
    #[arg(long)]
    engine: Option<String>,
    /// Print the on/off state of the entity at this pixel every tick: x,y or x,y=label. Repeatable.
    #[arg(long)]
    probe: Vec<String>,
    /// Write a PNG snapshot after a tick: tick:file.png. Repeatable.
    #[arg(long)]
    render: Vec<String>,
    /// Print these VMem words at the end, as hex and unsigned decimal: addr[,addr...]. Hex needs 0x.
    #[arg(long, value_delimiter = ',')]
    watch: Vec<String>,
    /// Print every change to a watched VMem word as it happens, with its tick. Without --watch, every VMem change.
    #[arg(long)]
    verbose: bool,
    /// Write all of VMem to a file at the end, one `0xADDRESS 0xVALUE` line per word.
    #[arg(long)]
    dump_vmem: Option<PathBuf>,
    /// Ignore Breakpoint inks.
    #[arg(long)]
    no_breakpoint: bool,
    /// Run even if the board uses inks that have no observed fixture yet, and let the assembler use
    /// language features that the manual states but no game save shows yet.
    #[arg(long)]
    allow_unverified: bool,
    /// Assemble this program into the board's memory before tick 0. Default: the board's own program
    /// (its Assembly Editor text, or <board>.vcbasm with External Editing), as the game does.
    #[arg(long)]
    asm: Option<PathBuf>,
    /// Assemble nothing: run the memory as the file holds it.
    #[arg(long)]
    no_asm: bool,
    #[arg(long)]
    clock_interval: Option<i64>,
    /// Seed for Random ink. Overrides the board's seed and its time-seed setting.
    #[arg(long)]
    seed: Option<i64>,
    /// How many ticks one Timer interval lasts. Required when the board has Timer ink.
    #[arg(long)]
    timer_ticks: Option<u64>,
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
        Command::Asm(args) => asm(args),
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

const EXTERNAL_SOURCE: &str = "External Editing source";

/// The program a board is assembled from: (text, name for messages, the file it came from).
/// `None` when there is nothing to assemble. A file's CRLF line ends are read as LF: the file stands for the
/// text as the game's Assembly Editor holds it.
fn read_source(board_path: &Path, board: &Board, asm: Option<&Path>, allow_unverified: bool) -> Result<Option<(String, String, Option<PathBuf>)>> {
    let file = match asm {
        Some(file) => Some(file.to_path_buf()),
        None if board.assembly_is_external => {
            if !allow_unverified {
                return Err(format!("{}: '{EXTERNAL_SOURCE}' is not verified against the game; pass --allow-unverified, or --asm <file>", board_path.display()));
            }
            let file = board_path.with_extension("vcbasm");
            if !file.exists() {
                return Err(format!("{}: External Editing is on and this file is missing: the game's result is not known (facts question 23r)", file.display()));
            }
            Some(file)
        }
        None => None,
    };
    let (text, name) = match &file {
        Some(file) => {
            let bytes = std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
            let text = String::from_utf8(bytes).map_err(|_| format!("{}: is not valid UTF-8", file.display()))?;
            (text.replace("\r\n", "\n"), file.display().to_string())
        }
        None => (board.assembly.clone(), format!("{}:assembly", board_path.display())),
    };
    Ok((!text.trim().is_empty()).then_some((text, name, file)))
}

fn vmem_dump(vmem: &[u32]) -> String {
    let mut text = String::with_capacity(vmem.len() * 22);
    for (address, value) in vmem.iter().enumerate() {
        text += &format!("{address:#010X} {value:#010X}\n");
    }
    text
}

fn asm(args: AsmArgs) -> Result<()> {
    let mut board = Board::read(&args.board)?;
    let source = read_source(&args.board, &board, args.asm.as_deref(), args.allow_unverified)?;

    // Check every output against every file this command reads, before anything is written.
    let listing_file = args.listing.as_deref().filter(|p| *p != Path::new("-"));
    let outputs: Vec<&Path> = [args.output.as_deref(), args.dump_vmem.as_deref(), listing_file].into_iter().flatten().collect();
    let (vcbmem, vcbasm) = (args.board.with_extension("vcbmem"), args.board.with_extension("vcbasm"));
    let mut inputs: Vec<&Path> = vec![&args.board, &vcbmem, &vcbasm];
    inputs.extend(args.asm.as_deref());
    check_outputs(&inputs, &outputs)?;
    for path in &outputs {
        check_overwrite(path, args.force)?;
    }

    let mut report = vcb::asm::Report::default();
    if let Some((text, name, file)) = &source {
        report = vcb::asm::assemble(text, name, &mut board, args.allow_unverified)?;
        if file.is_some() && args.asm.is_none() {
            report.unverified.push(EXTERNAL_SOURCE);
        }
        for feature in &report.unverified {
            eprintln!("warning: asm {feature} is not verified against the game");
        }
        println!("{}", report.summary(name));
        board.assembly = text.clone();
        board.assembly_is_external = false;
    }
    if let Some(path) = &args.output {
        write_file(path, board.to_vcb().as_bytes())?;
    }
    if let Some(path) = &args.dump_vmem {
        write_file(path, vmem_dump(&board.vmem).as_bytes())?;
    }
    match (&args.listing, listing_file) {
        (Some(_), None) => print!("{}", report.listing()),
        (_, Some(path)) => write_file(path, report.listing().as_bytes())?,
        _ => {}
    }
    Ok(())
}

fn gen(source: &Path, output: &Path, force: bool) -> Result<()> {
    let text = std::fs::read_to_string(source).map_err(|e| format!("{}: {e}", source.display()))?;
    let mut tb = vcb::textboard::parse(&text).map_err(|e| format!("{}: {e}", source.display()))?;
    let name = output.file_stem().and_then(|s| s.to_str()).ok_or("the output needs a file name")?;
    let (checklist_path, png_path) = (output.with_extension("checklist.md"), output.with_extension("png"));

    // Assembly programs named in the header: a plain file name next to the source, read with LF line ends.
    let program = |file: &str| -> Result<String> {
        if Path::new(file).components().count() != 1 || file == ".." {
            return Err(format!("{}: assembly file \"{file}\" must be a file name in the same folder", source.display()));
        }
        let path = source.with_file_name(file);
        Ok(std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?.replace("\r\n", "\n"))
    };
    if let Some(file) = &tb.assembly_file {
        tb.board.assembly = program(file)?;
    }
    let external = tb.external_file.as_deref().map(program).transpose()?;
    tb.board.assembly_is_external = external.is_some();
    let external_path = output.with_extension("vcbasm");

    let vcb_text = tb.board.to_vcb();
    let checklist = match (&tb.assembly_file, &external) {
        (None, None) => vcb::checklist(name, &vcb_text, &tb),
        _ => vcb::asm_checklist(name, &vcb_text, &tb, &[tb.board.assembly.as_str(), external.as_deref().unwrap_or("")]),
    };
    let labels: Vec<_> = tb.probes.iter().enumerate().map(|(i, p)| (i + 1, p.x, p.y)).collect();
    let png = vcb::render::render(&tb.board, None, &labels)?.to_png();

    // Check everything before writing anything.
    let mut outputs: Vec<&Path> = vec![output, &checklist_path, &png_path];
    outputs.extend(external.is_some().then_some(external_path.as_path()));
    check_outputs(&[source], &outputs)?;
    check_overwrite(output, force)?;
    check_overwrite(&png_path, force)?;
    if external.is_some() {
        check_overwrite(&external_path, force)?;
    }
    if let Ok(existing) = std::fs::read_to_string(&checklist_path) {
        if existing != checklist {
            return Err(format!("{}: already exists and differs; it may hold observations, so it is never overwritten", checklist_path.display()));
        }
    }
    write_file(output, vcb_text.as_bytes())?;
    write_file(&checklist_path, checklist.as_bytes())?;
    write_file(&png_path, &png)?;
    if let Some(text) = &external {
        let text = if tb.external_crlf { text.replace('\n', "\r\n") } else { text.clone() };
        write_file(&external_path, text.as_bytes())?;
    }
    println!("wrote {}, {} and {}", output.display(), checklist_path.display(), png_path.display());
    Ok(())
}

/// Fills every setting the command line left out from the config file.
fn apply_config(args: &mut RunArgs, path: &Path) -> Result<()> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let table: toml::Table = text.parse().map_err(|e: toml::de::Error| format!("{}: {}", path.display(), e.message()))?;
    let bad = |key: &str, want: &str| format!("{}: {key}: expected {want}", path.display());
    // A command-line --asm or --no-asm overrides both keys of the config file.
    let asm_on_command_line = args.asm.is_some() || args.no_asm;
    let list = |key: &str, value: &toml::Value| -> Result<Vec<String>> {
        let text = |v: &toml::Value| match v {
            toml::Value::String(s) => Ok(s.clone()),
            toml::Value::Integer(n) => Ok(n.to_string()),
            _ => Err(bad(key, "text or a number")),
        };
        match value {
            toml::Value::Array(items) => items.iter().map(text).collect(),
            one => Ok(vec![text(one)?]),
        }
    };
    for (key, value) in &table {
        let number = || value.as_integer().ok_or_else(|| bad(key, "a whole number"));
        let flag = || value.as_bool().ok_or_else(|| bad(key, "true or false"));
        let word = || value.as_str().map(String::from).ok_or_else(|| bad(key, "text"));
        match key.as_str() {
            "ticks" => args.ticks = args.ticks.or(Some(number()? as u64)),
            "quiescent" => args.quiescent = args.quiescent.or(Some(number()? as u64)),
            "seed" => args.seed = args.seed.or(Some(number()?)),
            "clock-interval" => args.clock_interval = args.clock_interval.or(Some(number()?)),
            "timer-ticks" => args.timer_ticks = args.timer_ticks.or(Some(number()? as u64)),
            "engine" => args.engine = args.engine.take().or(Some(word()?)),
            "dump-vmem" => args.dump_vmem = args.dump_vmem.take().or(Some(word()?.into())),
            "asm" if !asm_on_command_line => args.asm = Some(word()?.into()),
            "no-asm" if !asm_on_command_line => args.no_asm = flag()?,
            "asm" => drop(word()?),
            "no-asm" => drop(flag()?),
            "no-breakpoint" => args.no_breakpoint |= flag()?,
            "allow-unverified" => args.allow_unverified |= flag()?,
            "stats" => args.stats |= flag()?,
            "verbose" => args.verbose |= flag()?,
            "force" => args.force |= flag()?,
            "break-addr" if args.break_addr.is_empty() => args.break_addr = list(key, value)?,
            "watch" if args.watch.is_empty() => args.watch = list(key, value)?,
            "probe" if args.probe.is_empty() => args.probe = list(key, value)?,
            "render" if args.render.is_empty() => args.render = list(key, value)?,
            "break-addr" | "watch" | "probe" | "render" => {}
            _ => return Err(format!("{}: unknown key \"{key}\"", path.display())),
        }
    }
    Ok(())
}

fn parse_address(text: &str, vmem_len: usize) -> Option<usize> {
    let parsed = match text.strip_prefix("0x") {
        Some(hex) => usize::from_str_radix(hex, 16),
        None => text.parse(),
    };
    parsed.ok().filter(|a| *a < vmem_len)
}

fn run(mut args: RunArgs) -> Result<()> {
    // Ctrl-C ends the run after the current tick; the outputs are still written. The handler is installed
    // before the board is loaded, so an early Ctrl-C is not lost.
    let interrupt = Arc::new(AtomicBool::new(false));
    let flag = interrupt.clone();
    ctrlc::set_handler(move || flag.store(true, Ordering::Relaxed)).map_err(|e| format!("cannot install the Ctrl-C handler: {e}"))?;
    if let Some(config) = args.config.clone() {
        apply_config(&mut args, &config)?;
    }
    if args.asm.is_some() && args.no_asm {
        return Err("--asm and --no-asm cannot be used together".into());
    }
    let max_ticks = args.ticks.ok_or("--ticks is required (or `ticks` in the config file), so a cut-off run cannot look like a finished one")?;
    match args.engine.as_deref() {
        None | Some("v1") => {}
        Some("v2") => return Err("--engine v2: the v2 engine does not exist yet".into()),
        Some(other) => return Err(format!("--engine {other}: expected v1 or v2")),
    }
    let mut board = Board::read(&args.board)?;
    if let Some(n) = args.clock_interval {
        board.settings.clock_interval = n;
    }

    // Random: a time seed is printed so the run can be repeated with --seed.
    let has_random = board.inks.contains(&vcb::Ink::Random);
    match args.seed {
        Some(seed) => board.settings.random_seed = seed,
        None if board.settings.random_is_time_seed && has_random => {
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e| e.to_string())?;
            board.settings.random_seed = now.as_nanos() as i64;
            println!("seed: {}", board.settings.random_seed);
        }
        None => {}
    }

    // The program is assembled into the board's memory before tick 0, as the game does when a simulation starts.
    let mut unverified_asm: Vec<&'static str> = Vec::new();
    if args.no_asm {
    } else if !board.settings.vmem_enabled {
        if args.asm.is_some() {
            return Err("--asm: is_vmem_enabled is false, so there is no VMem to assemble into".into());
        }
        if board.assembly_is_external || !board.assembly.trim().is_empty() {
            println!("assembly text not assembled: VMem is disabled");
        }
    } else if let Some((text, name, file)) = read_source(&args.board, &board, args.asm.as_deref(), args.allow_unverified)? {
        let report = vcb::asm::assemble(&text, &name, &mut board, args.allow_unverified)?;
        unverified_asm = report.unverified.clone();
        if file.is_some() && args.asm.is_none() {
            unverified_asm.push(EXTERNAL_SOURCE);
        }
        for feature in &unverified_asm {
            eprintln!("warning: asm {feature} is not verified against the game");
        }
        println!("{}", vcb::asm::Report { unverified: unverified_asm.clone(), ..report }.summary(&name));
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
    for unit in vcb::ink::MODELLED {
        if board.inks.iter().any(|ink| ink.unit() == Some(unit)) {
            eprintln!("warning: {unit} is modelled, not game-exact");
        }
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
        if tick > max_ticks {
            return Err(format!("--render {spec}: tick {tick} is after the run's last tick {max_ticks}"));
        }
        renders.push((tick, file.into()));
    }
    let mut watch: Vec<usize> = Vec::new();
    for spec in &args.watch {
        watch.push(parse_address(spec, board.vmem.len()).ok_or(format!("--watch {spec}: not a VMem address"))?);
    }
    let mut break_addresses: Vec<(usize, Access)> = Vec::new();
    for spec in &args.break_addr {
        let (address, mode) = spec.split_once(':').unwrap_or((spec, "fetch"));
        let access = match mode {
            "fetch" => Access::Fetch,
            "write" => Access::Write,
            "any" => Access::Any,
            _ => return Err(format!("--break-addr {spec}: expected fetch, write or any after the colon")),
        };
        break_addresses.push((parse_address(address, board.vmem.len()).ok_or(format!("--break-addr {spec}: not a VMem address"))?, access));
    }
    if !board.settings.vmem_enabled && (!watch.is_empty() || args.dump_vmem.is_some() || !break_addresses.is_empty() || args.quiescent.is_some() || args.verbose) {
        return Err("is_vmem_enabled: is false, so there is no VMem to watch, dump or stop on".into());
    }

    let mut output_paths: Vec<&Path> = renders.iter().map(|(_, p)| p.as_path()).collect();
    output_paths.extend(args.dump_vmem.as_deref());
    let (vcbmem, vcbasm) = (args.board.with_extension("vcbmem"), args.board.with_extension("vcbasm"));
    let mut input_paths: Vec<&Path> = vec![&args.board, &vcbmem, &vcbasm];
    input_paths.extend(args.asm.as_deref());
    check_outputs(&input_paths, &output_paths)?;
    for path in &output_paths {
        check_overwrite(path, args.force)?;
    }

    let mut engine = Engine::new(&board, components, &Options { timer_ticks: args.timer_ticks })?;
    let mut render_error = None;
    let started = Instant::now();
    let stops = StopSet { max_ticks, breakpoint: !args.no_breakpoint, watch: break_addresses, quiescent: args.quiescent, interrupt: Some(interrupt) };
    let mut last_progress = (Instant::now(), 0u64);
    let report = engine.run_until(&stops, |engine| {
        // With --stats: a progress line every 5 seconds. The clock is read only every 65,536 ticks.
        if args.stats && engine.tick() & 0xffff == 0 && last_progress.0.elapsed().as_secs() >= 5 {
            let speed = (engine.tick() - last_progress.1) as f64 / last_progress.0.elapsed().as_secs_f64();
            eprintln!("tick {}  TPS: {speed:.0}", engine.tick());
            last_progress = (Instant::now(), engine.tick());
        }
        if args.verbose && engine.vmem_changed() {
            if let (_, Some(address)) = engine.vmem_access() {
                if watch.is_empty() || watch.contains(&address) {
                    let value = engine.vmem()[address];
                    println!("tick {}: {address:#010X} = {value:#010X} ({value})", engine.tick());
                }
            }
        }
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

    let reasons: Vec<String> = report.reasons.iter().map(|reason| match (*reason, report.watch_address) {
        ("address-watch", Some(address)) => format!("address-watch {address:#010X}"),
        _ => reason.to_string(),
    }).collect();
    let mut line = format!("stopped at tick {}: {}", report.tick, reasons.join(", "));
    if !unverified.is_empty() || !unverified_asm.is_empty() {
        let units: Vec<String> = unverified.iter().map(|(unit, _)| unit.to_string()).chain(unverified_asm.iter().map(|f| format!("asm {f}"))).collect();
        line += &format!(" (unverified: {})", units.join(", "));
    }
    println!("{line}");
    for address in watch {
        let value = engine.vmem()[address];
        println!("{address:#010X} = {value:#010X} ({value})");
    }
    if let Some(path) = &args.dump_vmem {
        write_file(path, vmem_dump(engine.vmem()).as_bytes())?;
    }
    if let Some((tick, path)) = renders.iter().find(|(tick, _)| *tick > report.tick) {
        return Err(format!("--render {tick}:{}: the run stopped at tick {} before that tick", path.display(), report.tick));
    }
    if args.stats {
        eprintln!("ticks: {}  time: {elapsed:.3} s  TPS: {:.0}", report.tick, report.tick as f64 / elapsed.max(1e-9));
    }
    Ok(())
}
