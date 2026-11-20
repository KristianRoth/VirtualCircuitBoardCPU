# vcb: a tick-exact engine for Virtual Circuit Board

Runs `.vcb` saves of the Steam game Virtual Circuit Board (Update 1.0.1) outside the game, tick for tick like the game does. It runs the 32-bit CPU in this repo through Advent of Code day 1 part 1 in about 8 seconds, stopping on the same tick as the game with identical memory.

- What the engine must do and why: [`../specs/vcb-engine.md`](../specs/vcb-engine.md)
- How v1 works, its milestones and decisions: [`../specs/vcb-engine-v1.md`](../specs/vcb-engine-v1.md)
- Everything observed about the game and its file format: [`../specs/vcb-game-facts.md`](../specs/vcb-game-facts.md)

## Build and test

Rust stable, pinned in `rust-toolchain.toml`.

```
cargo build --release
cargo test                         # about 30 seconds
cargo test --release -- --ignored  # the CPU run against the game's save, about 8 seconds
```

**The fastest build** is made with profile-guided optimisation, which `cargo build --release` alone does not do:

```
sh tools/build-pgo.sh              # about 70 seconds; needs `rustup component add llvm-tools` once
```

It builds a recording version of the engine, runs day 1 part 1 on the engine's own fixture board with it, and builds `target/release/vcb` again with what was recorded. The result is about 1.12 times faster on every board tried. A plain `cargo build --release` afterwards replaces it with the ordinary build, which is correct and that much slower. `cargo test` is not affected.

On the author's Mac two settings are needed, because `cargo` is not on the PATH and the default macOS 27 SDK breaks linking:

```
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
export SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX26.5.sdk
```

## Commands

```
vcb info <board.vcb>               size, settings, pixel count per ink, what is not verified
vcb info --adjacency <board.vcb>   which inks touch (for pairs involving Read, Write, Cross, Tunnel, Mesh, Bus)
vcb gen <board.txt> -o <out.vcb>   text-art board -> .vcb, plus <out>.checklist.md and <out>.png
vcb asm <board.vcb> --asm <prog>   assemble a program into the board's memory without running (see "Running a new program")
vcb run <board.vcb> --ticks <n>    simulate
```

`vcb run` options:

| Option | Meaning |
|---|---|
| `--ticks <n>` | Maximum ticks. Required, here or in the config file. |
| `--config <run.toml>` | Settings file; keys are the long flag names. Flags on the command line win. |
| `--break-addr <addr>[:fetch\|write\|any]` | Stop when VMem loads from (fetch, the default) or writes to that address. Repeatable. |
| `--quiescent <n>` | Stop once VMem has not changed for n ticks. |
| `--no-breakpoint` | Ignore Breakpoint ink. |
| `--watch <addr>[,<addr>…]` | Print these VMem words at the end, as hex and decimal. |
| `--verbose` | Print every change to a watched word as it happens, with its tick. Without `--watch`, every VMem change. |
| `--dump-vmem <file>` | Write all of VMem at the end, one `0xADDRESS 0xVALUE` line per word. |
| `--probe <x,y>[=<label>]` | Print the on/off state of the entity at that pixel every tick. Repeatable. |
| `--render <tick>:<file.png>` | Picture of the board after that tick; entities that are off are darkened. Repeatable. |
| `--seed <n>` | Seed for Random ink (overrides the board's seed and its time-seed setting). |
| `--clock-interval <n>` | Overrides the board's clock interval. |
| `--timer-ticks <n>` | Ticks per Timer interval. Required when the board has Timer ink. |
| `--stats` | Ticks, time and ticks per second at the end; a progress line every 5 seconds. |
| `--force` | Allow overwriting output files. |
| `--allow-unverified` | Run even if the board uses something with no observed fixture. (For the assembler it changes nothing today: every feature has been observed.) |
| `--asm <file.vcbasm>` | Assemble this program before tick 0 instead of the board's own. |
| `--no-asm` | Assemble nothing; run the memory as the file holds it. |
| `--engine v1` | Run the reference engine. The default is v2, the fast engine, which shows the same VMem events, stop and final memory. With `--probe` or `--render` a run is done by v1 (it says so). |
| `--verify` | Run on v2, then again on v1, and fail at any difference. Use it for the first answer of a new program. |
| `--replay-cache <file>` | Keep what v2's replay learns in this file: it is read before the run if it is there and written after. A second run of a program starts with every step known (day 4 part 2: 12.5 s the first time, 1.3 s the second). The file is used only by the same build of the engine on the same board; anything else is replaced. It can be as large as the memory replay may use. |
| `--replay-memory <MiB>` | How much memory replay may use (default 2048). When it is full the table is emptied and learnt again; a table of 32 MiB costs about 6% of the speed on the longest program. |

The run ends on the first tick where a stop condition holds and prints every one that held, for example `stopped at tick 7: breakpoint, max-ticks`. Ctrl-C stops after the current tick with reason `interrupted` and still writes the requested outputs.

Example: the CPU fixture.

```
./target/release/vcb run tests/boards/cpu-aoc1p1.vcb --ticks 60000000 --stats --watch 0x100,0x101
```

## Running a new program on the CPU

`vcb run` assembles the board's program into its memory before tick 0, as the game does when a simulation starts (spec: `../specs/vcb-assembler.md`). The program is, in this order: the file given with `--asm`; `<board>.vcbasm` when the board has External Editing on; the board's Assembly Editor text. `--no-asm` assembles nothing and runs the memory as the file holds it. The input files are never changed.

```
./target/release/vcb run <cpu board> --asm <program.vcbasm> --ticks <n> --watch <addresses>
```

For example, Advent of Code day 1 part 2 on the CPU (about 12 s; `AoC2025/` is a separate repository nested in this folder):

```
./target/release/vcb run ../AoC2025/cpu-32bit-autorun-aoc1p1.vcb --asm ../AoC2025/days/day1/part2.vcbasm --ticks 500000000 --watch 0x100
```

`vcb asm <board> [--asm <program>]` assembles without running. `-o <new.vcb>` writes the board with the assembled memory and the program as its Assembly Editor text, `--dump-vmem <file>` the memory, `--listing <file>` (`-` prints it) the address, word, line and statement of every stored word, then the labels and pointers. With no output it only checks the program.

The assembler accepts what the game has been seen to assemble and refuses what it has been seen to reject (`the game rejects this`). A few corner cases the game has not been asked about are refused with a letter of question 23 in `../specs/vcb-game-facts.md`. All such problems are listed in one run. A `.vcbasm` file may have Windows line ends. Note that the game rejects a `;` on the line of a macro definition.

Assemble onto a clean board: words of an earlier program that the new one does not overwrite stay, and the `assembled` line then says how many words it replaced.

## Layout

| Path | What |
|---|---|
| `src/ink.rs` | Ink types, colour table, the lists of verified and modelled units |
| `src/format.rs`, `src/board.rs` | `.vcb` reader and writer, `.vcbmem` reader |
| `src/textboard.rs` | Text-art boards (the character legend is the `LEGEND` table) |
| `src/netlist.rs` | Board → entities and connections (traces, components, buses, pins; cross, tunnel, mesh, bus rules) |
| `src/sim.rs` | The engine: dirty list + recompute, clocks, Random (MT19937), VMem, stop conditions |
| `src/render.rs` | PNG snapshots |
| `src/asm.rs` | The assembler and its gate (`VERIFIED` lists the manual-only features a game save has since shown) |
| `src/main.rs` | Command line |
| `boards/*.txt` | Text-art sources of the test boards |
| `tests/boards/` | Observed fixtures: `<name>.vcb`, `<name>.observed.md` (the checklist the user filled in), `<name>.after.vcb` (the game's save after a run) |
| `tests/evidence/` | Game saves that back a fact in the facts file but are not used by a test |
| `tests/` | `reader.rs`, `cli.rs`, `model.rs` (engine against the spec's rules), `fixtures.rs` (engine against the game), `asm.rs` (assembler against the game's day 1 memory, and its gate) |
| `tools/board-generators/` | Python scripts that wrote the larger `boards/*.txt` files, and `asm_probes.py` for the assembler probe programs (`boards/asm-*.vcbasm`). Run from `engine/`. |
| `tools/random-search/` | The scripts that identified the game's random generator from the recordings. Run from the repo root. |
| `examples/diff_boards.rs` | Shows the pixels that differ between two boards |
| `examples/trace.rs` | Inspects a board: text art of an area, what an entity connects to, a bus and the traces touching it, and the on/off changes of chosen entities during a run |

## How correctness is established

Ground truth is the game. For each rule there is a board that was stepped in the game by hand, or run in the game and saved with Persistent Memory. The fixture tests compare the engine with those observations tick by tick and word by word. A unit (an ink, or VMem) is only accepted by `vcb run` once such a fixture exists; `vcb info` lists anything that isn't. Timer is the one unit that is "modelled": the game's timers follow the wall clock, so it cannot be tick-exact.

## Speed (stage 1 of v2, 2026-11-19)

This table is where stage 1 ended. The engine has since become about 2.3 times faster again (day 4 part 2 in 6.4 s): the current figures, and how to run the profiling and measuring tools, are in `../specs/vcb-engine-v2.1-end.md`.

Every benchmark program run to its end on both engines in one session (one run each; `AoC2025` at commit `9e8bd62`; the engine at attempt 15). Both engines stop at the same tick with the same output on every row.

| Program | Ticks | v1, M ticks/s | v2, M ticks/s | v2 against v1 | v1 time | v2 time |
|---|---|---|---|---|---|---|
| day1/part1 | 43,146,512 | 6.20 | 69.0 | 11.1 | 7.0 s | 0.6 s |
| day1/part2 | 70,339,472 | 6.00 | 64.0 | 10.7 | 11.7 s | 1.1 s |
| day2/part1 | 43,495,312 | 5.99 | 147.9 | 24.7 | 7.3 s | 0.3 s |
| day2/part2 | 75,753,616 | 5.85 | 196.8 | 33.6 | 12.9 s | 0.4 s |
| day3/part1 | 170,904,144 | 6.15 | 152.7 | 24.8 | 27.8 s | 1.1 s |
| day3/part2 | 419,669,712 | 6.37 | 128.7 | 20.2 | 65.9 s | 3.3 s |
| day4/part1 | 481,327,888 | 5.80 | 97.7 | 16.8 | 83.0 s | 4.9 s |
| day4/part2 | 1,339,421,264 | 5.81 | 94.4 | 16.2 | 230.5 s | 14.2 s |
| sample-primes | 100,000,000 (limit) | 5.78 | 495.1 | 85.7 | 17.3 s | 0.2 s |
| sample-compact-primes | 100,000,000 (limit) | 13.45 | 52.6 | 3.9 | 7.4 s | 1.9 s |

Geometric mean: v2 is **18.4 times** v1. Peak memory of the longest run (day 4 part 2 on v2): 699 MB. Times are ticks divided by speed; start-up (about 0.07 s) is not in them.

How v2 got there, attempt by attempt, is in `../specs/vcb-engine-v2-log.md`; what it does and what is still open, in `../specs/vcb-engine-v2.1.md`.
