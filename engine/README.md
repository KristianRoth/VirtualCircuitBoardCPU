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
| `--allow-unverified` | Run even if the board uses something with no observed fixture, and let the assembler use features only the manual states. |
| `--asm <file.vcbasm>` | Assemble this program before tick 0 instead of the board's own. |
| `--no-asm` | Assemble nothing; run the memory as the file holds it. |
| `--engine v1` | Only v1 exists. |

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

The assembler accepts only what a game save has shown. A feature that only the manual states needs `--allow-unverified`; anything else is refused with a letter of question 23 in `../specs/vcb-game-facts.md`. All such problems are listed in one run. A `.vcbasm` file may have Windows line ends.

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
| `tools/board-generators/` | Python scripts that wrote the larger `boards/*.txt` files. Run from `engine/`. |
| `tools/random-search/` | The scripts that identified the game's random generator from the recordings. Run from the repo root. |
| `examples/diff_boards.rs` | Shows the pixels that differ between two boards |

## How correctness is established

Ground truth is the game. For each rule there is a board that was stepped in the game by hand, or run in the game and saved with Persistent Memory. The fixture tests compare the engine with those observations tick by tick and word by word. A unit (an ink, or VMem) is only accepted by `vcb run` once such a fixture exists; `vcb info` lists anything that isn't. Timer is the one unit that is "modelled": the game's timers follow the wall clock, so it cannot be tick-exact.
