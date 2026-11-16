# VCB simulation engine

## Problem & goal
The user has a 32-bit computer built in Virtual Circuit Board (VCB, Steam app 1885690), saved as `engine/tests/boards/cpu-32bit-manual-start.vcb`, and wants to solve this year's Advent of Code with it. The game's simulator is too slow for that. openVCB (cloned in `openVCB/`) only reads the 2022 Godot `.tres` save format, not the current one. The goal is a new, fast simulation engine that loads current `.vcb` files and runs them **tick-exactly** like the game.

**Status (2026-11-16):** v1 is complete (milestones M0–M7 in `vcb-engine-v1.md`), including the final boss in a first form: the CPU runs AoC day 1 part 1 exactly like the game, with the program assembled by the game. Still Later: the assembler, v2, VInput/VDisplay, a viewer, the openVCB patch and benchmark.

**Game facts:** the file format, colour→ink table, VMem layout and timing, and everything observed in the game live in [`vcb-game-facts.md`](vcb-game-facts.md) ("the facts file"). This spec is the framework: goals, scope, interface, test ladder and rollout. The detailed design of the v1 engine is in `vcb-engine-v1.md`.

**Game version:** VCB **Update 1.0.1**. The colour→ink table, ink semantics, fixtures and file-format notes are taken from this version. Every fixture records the game version it was made with. A game update means re-checking every fixture before anything else changes.

Priorities, in order:
1. **Correct:** the engine produces the same state as the game, proven on a ladder of circuits that grow in complexity. The last rung (the "final boss") is the AoC program running on the 32-bit CPU.
2. **Learning:** understanding VCB internals is part of the value; the code should stay readable.
3. **Speed:** beat openVCB, which claims 2–4x the game's speed. There is no fixed TPS target and no minimum speed for v1.
4. **Viewer:** later, out of scope.

## Users & roles
A single user (the author) runs it locally from the CLI. There is no auth or multi-user concern. The user builds test boards in the game on request.

## Scope
### In scope (v1)
- Rust workspace in `./engine/`: a library crate plus a thin CLI binary.
- Reader for `.vcb` **file_version 2** (JSON) files.
- **v1 engine:** a single-threaded, event-driven reference simulator, written to be easy to read, change and test. Speed is secondary.
- Every current VCB logic ink: traces (all colours), read, write, cross, buffer, AND, OR, NAND, NOR, NOT, XOR, XNOR, latch, clock, timer, random, breakpoint, LED, bus/tunnel/mesh/wireless, filler and annotation (no logic). The exact list is confirmed from the in-game User Guide.
- Memory-mapped VMem (address, data and control pins per `vmem_settings`), loaded from `vmem_data`.
- Stop conditions: max ticks, Breakpoint ink, address watch, and VMem quiescent for N ticks (a debug aid).
- Outputs, each configurable: print watched VMem addresses, dump all of VMem to a file, stats (ticks, wall time, TPS), and progress output while running. Writing state back to a `.vcb` was dropped, because the game neither saves nor loads live state (see Data).
- Configuration layers: settings in the `.vcb`, overridden by a TOML run file, overridden by CLI flags.
- A minimal openVCB patch: CMake build on macOS plus a v2 JSON reader for the basic inks it already knows. Its correctness is unverified, so it is **an untrusted second opinion, never an oracle**. It is used for speed comparison and as a hint when a basic-gate fixture fails.
- The test-circuit ladder (see Rollout), built in the game by the user.

### Out of scope
- An assembler (`assembly` field / `.vcbasm`) is a separate later project. The game assembles only at simulation start and never saves the result, so v1 runs whatever is in `vmem_data`. Test programs are written into `vmem_data` by script.
- The old `.tres` format in the new engine. It is detected by its `[gd_resource` header and rejected with a clear message.
- A graphical viewer or editor. A viewer may come later. openVCB's licence forbids shipping openVCB code together with an editor; the engine does not copy openVCB code, so this does not bind the engine.

### Later / TODO
- VInput (keyboard) and VDisplay.
- **v2 engine:** optimised behind the same `Simulation` trait and CLI (multithreading, cache reordering, anything profiling shows). It must stay tick-exact and is checked against v1 with differential tests.
- Viewer.

## Behavior
1. `vcb run board.vcb [--config run.toml] [flags]`.
2. Read the file. A `[gd_resource` header is rejected as the old format. Otherwise parse the JSON and require `file_type == "RF_VCB_PROJECT"` and `file_version == 2`.
3. Decode `layers[0]` (logic) as a **blob** (see Data). The result is width×height RGBA pixels, 2048×2048 in the current file. Map every colour to an **ink type** using the colour→ink table. Pixels never encode on/off state. The many trace colours all mean "trace". `layers[1..]` are ignored.
4. Decode `vmem_data` as a blob into VMem. Read `vmem_settings`, `clock_interval`, `timer_interval`, `random_seed`/`random_is_time_seed` and `is_vmem_enabled`.
5. Preprocess: flood-fill connected trace and ink regions into components, and build the read/write graph between them.
6. Set the starting state. The file holds no live state, so every run starts from reset: all components off, VMem as stored in `vmem_data`. Which components the game evaluates on the first tick (for example, NOT gates turning on) is open question 6.
7. Simulate tick by tick with the game's semantics: gate delay, latch, clock and timer behaviour, and the per-tick VMem read/write timing.
8. After each tick, check every stop condition. The run ends on the first tick where any condition holds. **Every** condition that holds on that tick is reported, in this order: `breakpoint`, `address-watch 0x…`, `quiescent`, `max-ticks`. Example output: `stopped at tick 81234: breakpoint, max-ticks`.
9. While running, print progress to stderr every 5 s when `--stats` is on (tick, TPS). On Ctrl-C, stop cleanly with reason `interrupted` and still produce the configured outputs.
10. Produce the configured outputs.

**Address watch:** `--break-addr X[:mode]` fires on a tick in which the VMem address pins equal `X` at the moment VMem samples them. `mode` is `fetch` (a read access, the default), `write`, or `any`. Addresses are VMem word addresses, and the word width is the data width in `vmem_settings`.

## Edge cases
- These are all hard errors, and the message names the field, or the pixel `x,y #rrggbbaa`:
  - unknown colour, `file_version` or `file_type`;
  - a missing field;
  - malformed base64 or zstd;
  - a blob trailer that doesn't match the decompressed size;
  - a `vmem_data` blob that is not 4,194,304 bytes (or 4,194,305, see Data). With `vmem_is_external: true` the inline blob is ignored.
- `vinput_is_enabled: true` or `vdisplay_is_enabled: true` are hard errors in v1. Each needs a component that v1 doesn't simulate yet. `vmem_is_external: true` is supported: memory is read from `<project>.vcbmem`.
- Board sizes other than 2048×2048 come from the trailer, not a hardcoded value. Width×height×4 is computed with checked arithmetic and capped at 64 MiB before anything is allocated.
- `random_is_time_seed: true` (the current file has this) uses a time-based seed, as the game does, and prints `seed: <n>`. `--seed <n>` reproduces that run. Tests and fixtures always use a fixed seed.
- `is_vmem_enabled: false` runs with no VMem. Address watch, `--watch` and `--dump-vmem` then fail with an error naming `is_vmem_enabled`.
- Max ticks must be given by `--ticks` or `ticks` in the TOML. A missing value is an error, so a truncated run can't look like a success.

## Data
- **Formats:** the blob format, logic layer, colour→ink table, VMem layout, external VMem and pin layout are in the facts file.
- **VMem size exception:** a VMem blob of 4,194,305 bytes (seen in `engine/tests/boards/cpu-32bit-manual-start.vcb`) is accepted, and the extra byte is ignored.
- **VMem word 0:** until facts question 8 is answered, the engine sets word 0 to 0 on load and warns if the file had a non-zero value there.
- **No live state:** saves never contain component state, so every run starts from reset. VMem written by the circuit is saved only inside the Persistent Memory region (see the facts file); VMem fixtures use such saves as ground truth.
- **Input:** `.vcb` v2 JSON. The engine never changes the input file.
- **Write safety:**
  - Refuse when `--dump-vmem` resolves to the input file.
  - Refuse to overwrite any existing file unless `--force` is given.
  - Write to a temporary file, then rename it into place.
- **VMem dump:** one line per address over the full VMem range, in ascending order, formatted as `0x%08X 0x%08X`.
- **Fixtures:** `engine/tests/boards/<name>.vcb` plus `<name>.expected.toml`, and for VMem boards `<name>.after.vcb` (the game's save after the run, see the v1 spec). The TOML holds `game_version`, `ticks`, `seed`, the stop rule, and the expected VMem words and/or the expected on/off state of named components at given ticks (observed in the game). Real AoC puzzle inputs may be committed.

## Interface
CLI (flags override the TOML file, which overrides the `.vcb`):
```
vcb run <board.vcb>
  --config <run.toml>
  --ticks <N>                    # required: max ticks
  --break-addr <addr>[:fetch|write|any]
  --no-breakpoint                # ignore Breakpoint inks
  --quiescent <N>                # stop after N ticks of unchanged VMem (debug)
  --watch <addr>[,<addr>...]     # print these words at the end, as hex plus unsigned decimal
  --dump-vmem <file>
  --force                        # allow overwriting an existing dump file
  --seed <n>
  --clock-interval <n> --timer-interval <n>
  --stats                        # final stats plus progress every 5 s
  --engine v1|v2                 # v2 is an error until it exists
```
The v1 spec adds `vcb info`, `vcb gen`, `--probe`, `--render`, `--timer-ticks` and `--allow-unverified`, and lists where v1 overrides this spec ("Overrides of the framework spec").
- `run.toml` uses the long flag names as keys.
- Fields that can be overridden from the `.vcb`: `clock_interval`, `timer_interval`, `random_seed` and `random_is_time_seed`. `simulation_speed_ticks` is ignored, because it is the game's UI speed setting.

Library:
- `Board` is the parsed file.
- `Components` is the preprocessing output and owns the pixel→component map and a canonical component order.
- `trait Simulation` has:
  - `step()`
  - `run_until(&StopSet) -> StopReport`: each engine checks the stops inside its own loop.
  - `vmem()`
  - `state() -> Vec<bool>` in canonical component order. Used by the v1/v2 differential tests.

## Integrations
- **openVCB:** the engine may read openVCB's source to understand the format and algorithms, but must not copy it. The engine is the user's own code under its own licence. The openVCB patch is GPL-3 and lives outside `./engine` as `tools/openvcb.patch`, pinned to openVCB commit `4903fa8`.
- **Ink semantics:** the in-game User Guide, the VCB wiki and community docs, confirmed by the user's test boards. Those boards are the only source of ground truth.
- **Crates:** `serde_json`, `base64`, `zstd`, `clap`, `toml`.

## Errors
- A reader error exits non-zero with exactly one stderr line naming the field or pixel.
- A stop on max ticks with no other condition firing is a normal exit (0). The stop reason is always printed.
- No retries; everything runs locally.

## Security & privacy
Input files are trusted local files. The engine still decodes with bounds: checked arithmetic, size caps before allocating, and no unchecked indexing. A bad file produces an error instead of a crash. Write safety is covered under Data.

## Performance
- v1: no speed target. Correctness comes first.
- v2: as fast as possible. It is compared with the patched openVCB and the game, and `--stats` reports TPS.
- Benchmark method: the median of 5 runs of 10⁶ ticks on a named fixture.

## Acceptance criteria
Reader and errors
- [ ] `vcb run AoC2025/cpu-32bit-manual-start.vcb --ticks 0` parses the file without any unknown-ink error.
- [ ] Each of these cases has a test that checks for a non-zero exit, exactly one stderr line, and the field or `x,y #colour` in that line:
  - old `.tres` file;
  - unknown `file_type`;
  - unknown `file_version`;
  - missing field;
  - bad base64;
  - bad zstd;
  - trailer size mismatch;
  - unknown colour;
  - `vinput` or `vdisplay` enabled.
- [ ] A 64×64 fixture loads and simulates.
- [ ] With `is_vmem_enabled: false`, `--watch` and `--dump-vmem` exit non-zero and name `is_vmem_enabled`.

Correctness ladder (each rung's fixtures pass before the next rung starts)
- [ ] Every fixture of every ladder rung matches its `expected.toml`. Each fixture's ground truth comes from the game (Update 1.0.1).
- [ ] With a fixed seed, two runs produce identical state and VMem.
- [ ] (After the assembler project) Final boss: the AoC board, with the program and input named in its fixture and written into `vmem_data`, produces the `count` value at `0x0100` recorded from the game. It stops using the stop rule recorded in its fixture.

Stops and outputs
- [ ] For each of `--break-addr` in `fetch`, `write` and `any` mode, a fixture stops at its known tick and prints `address-watch 0x…`.
- [ ] A Breakpoint fixture stops at its known tick. With `--no-breakpoint` it runs to `--ticks`.
- [ ] `--quiescent N` stops N ticks after the last VMem change.
- [ ] When a breakpoint and max ticks fire on the same tick, both are printed.
- [ ] Running without `--ticks` is an error.
- [ ] Ctrl-C prints `interrupted` and still writes the configured outputs.
- [ ] Dumping VMem at `--ticks 0` gives the same result as decoding `vmem_data` directly, except word 0 (see Data).
- [ ] Config precedence: with `clock_interval` set to 1 in the `.vcb`, 3 in the TOML and 5 on the CLI, the effective value is 5. Without the flag it is 3, and without the TOML it is 1.

- [ ] `--dump-vmem` set to the input path fails. Writing to an existing file fails unless `--force` is given.

Speed and v2
- [ ] `--stats` reports ticks, time and TPS.
- [ ] The patched openVCB builds on macOS and the benchmark script records its TPS.
- [ ] (v2) A seeded random-board generator produces at least 1000 boards. On those, plus every fixture, v1 and v2 states are equal after every tick for 10,000 ticks, and a failing case prints its seed.

## Rollout
0. **Prerequisites in the game (owner: the user):**
   - ~~Check step-and-save~~ and ~~assemble-and-save~~: **done 2026-11-13.** Neither works, so ground truth comes from observation in the game and programs go in by script (open questions 1 and 2).
   - Record the in-game TPS of the 32-bit board on the Windows PC (still open).
1. **Format check:** decode every field of `engine/tests/boards/cpu-32bit-manual-start.vcb`, list every colour found, and identify the trailer fields.
2. **Colour→ink table:** from the User Guide and wiki, confirmed with ladder rung A.
3. **v1 engine**, built up the ladder one rung at a time. Each rung's boards are built by the user in the game and committed as fixtures:
   - **A. One ink per board:** a trace plus a single gate for each gate type; latch, clock (including mid-period), timer, random, breakpoint and LED; bus, tunnel, mesh and wireless; the load-convention board (open question 6).
   - **B. Small combinations:** ring oscillator, SR latch, D flip-flop, 4-bit counter, 4-bit adder.
   - **C. VMem boards:** write a value to an address; read it back; write to and read from address 0 (open question 8); a counter that stores into VMem. These pin down VMem timing and the meaning of `vmem_settings`.
   - **D. CPU subsets:** the ALU on its own, then the CPU running tiny hand-encoded programs (store a constant; a loop of 10), written into `vmem_data` by script.
   - **E. Final boss:** the AoC day 1 board. This needs the assembler project (Later), or a hand-written encoding of the program.
4. **openVCB patch** (`tools/openvcb.patch`) and the benchmark script.
5. **Later:** the v2 engine with differential tests, then VInput/VDisplay, the assembler and a viewer.

## Open questions
All open questions are about the game and live in the facts file (questions 3, 4, 6–21). References to "open question N" in this spec mean that list. Questions 1, 2 and 5 are answered there.

## Decision log
- Success metric → correct results, proven up a ladder of circuits with the AoC program as the final boss; then learning, then speed, then a viewer.
- Language → Rust: C-level speed with readable, safe code (C, Zig and Go were considered).
- openVCB → the engine may read its code but must not copy it. The user first chose truly clean-room, then relaxed it to no copying, because the openVCB patch stays in scope.
- Spec split (2026-11-15) → game facts moved to `vcb-game-facts.md`; the v1 design and its decisions are in `vcb-engine-v1.md`.
- Assembler → out of scope. First plan: the user assembles in the game and saves. That turned out impossible (2026-11-13), so programs go into `vmem_data` by script and the assembler stays a later project.
- Halt → max ticks, Breakpoint ink, address watch, plus VMem-quiescent as a debug aid. The engine stays a general circuit simulator.
- Outputs → watched addresses, VMem dump and stats, all configurable. Write-back was dropped later (see below).
- Baseline → the user builds test boards in the game.
- Fidelity → tick-exact.
- Components → every current logic ink plus VMem. VInput and VDisplay go on the TODO list.
- Ink semantics → User Guide, wiki and docs, plus user-built test boards.
- Oracle → still to be checked in the game (open question 2); the fallback is to compare end states only.
- Speed target → beat openVCB. No v1 floor; the review proposed one and the user declined.
- Location → `./engine/` in this repo.
- Interface → library crate plus a thin CLI.
- openVCB test bed → minimal patch: macOS build plus a v2 reader. Treated as an untrusted second opinion, never an oracle.
- Configuration → CLI flags plus a TOML run file, over the `.vcb` settings.
- Performance strategy → v1 is a simple single-threaded event-driven engine; v2 is optimised and multithreaded behind the same CLI.
- v1 vs v2 → differential tests.
- Write-back → first required exact resume for every ink. **Dropped on 2026-11-13** after testing showed saves hold no live state and the game always simulates from reset.
- Unexpected input → fail loudly with the location.
- Saved VMem (review, S1) → the user re-saves from the game.
- Detecting AoC `END` (review, S2) → the AoC program is the final boss, after a ladder of simpler circuits; address watch is defined as above.
- Scope cuts (review, S11–S13) → none; everything stays in v1.
- Game version (review, S9) → pinned to Update 1.0.1 (confirmed in the game on 2026-11-13).
- AoC inputs in git (review, S19) → allowed, at the user's choice.
- Clear fixes from the review → applied: write safety, blob/trailer format, stop rule, required `--ticks`, VInput/VDisplay/external-VMem errors, progress and Ctrl-C, patch location, trait shape, output formats, testable criteria, nits.
