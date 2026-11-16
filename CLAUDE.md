# VirtualCircuitBoardCPU

A 32-bit CPU built in the Steam game Virtual Circuit Board (VCB), and `engine/`: a Rust engine that runs VCB saves tick for tick like the game. The aim is to solve Advent of Code on the CPU, faster than the game allows.

## Read these first
- `AoC2025/` and `AoC2026/` are **separate git repositories nested in this folder** and git-ignored here (the user's choice, 2026-11-16). `AoC2025/` holds the CPU boards (`cpu-32bit-manual-start.vcb`, formerly `32-bit-alu-try.vcb`; `cpu-32bit-autorun-aoc1p1.vcb`, formerly `CPU.vcb`) and the programs in `days/day1/part1.vcbasm` and `part2.vcbasm`; see its README. Commit there with `git -C AoC2025 …`, never from this repo. The engine does not depend on them: its tests use their own copy of the CPU board in `engine/tests/boards/cpu-32bit-manual-start.vcb`.
- `specs/vcb-engine-v1.md`: the v1 design, the milestone table with status (Rollout), open questions, and the decision log. This is the plan being followed.
- `specs/vcb-game-facts.md`: everything observed about the game and the `.vcb` format, and every open question about the game. New facts about the game go here, with the date and the board that showed them.
- `specs/vcb-engine.md`: the framework spec (goals, CLI, stop rules). The v1 spec lists where it overrides this.
- `engine/README.md`: build, commands, layout, how to run a program on the CPU.
- `manual/*.md`: the game's User Guide, transcribed. Git-ignored on purpose (the game's copyrighted text, public repo). Do not commit it.

## State (2026-11-16)
- Milestones M0–M7 of v1 are done. Every ink and VMem is verified against the game; Timer is "modelled".
- The CPU runs AoC day 1 part 1 to its Breakpoint at the game's tick with identical memory: `cargo test --release -- --ignored`, about 8 s, about 5.5 million ticks per second.
- Candidates for what comes next (the user decides): an assembler in the engine (the game's assembly language is in `manual/08-assembly.md`), so programs need not be run in the game first; the v2 engine (optimised, checked against v1 with differential tests); AoC day 1 part 2 (`AoC2025/days/day1/part2.vcbasm`); the openVCB patch and benchmark; choosing a licence for `engine/` before pushing.

## Building
`cargo` is not on the PATH and the default macOS 27 SDK breaks linking:
```
export PATH="/opt/homebrew/opt/rustup/bin:$PATH" SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX26.5.sdk
cd engine && cargo test
```
Use `./target/release/vcb` for long runs; the debug build is far slower.

## Rules of this project
- **The game is the ground truth.** If the engine disagrees with an observation, the engine is wrong until the user has re-observed that case. openVCB (`openVCB/`, git-ignored clone) may be read for ideas but never copied, and is not an oracle.
- **Never guess a game rule silently.** A rule that is not observed is marked as a model in the spec, has a question number in the facts file, and gets a board. A pattern the rules don't define is an error in the engine.
- **A model change must keep every earlier fixture passing**, and the changed rule is edited in the v1 spec and the facts file in the same commit.
- **Boards for the user go in `saves/pending/`**, never under `engine/`. The whole `saves/` folder is git-ignored: it is the user's scratch space for the game. After observation, what is needed is moved to `engine/tests/boards/` (fixtures) or `engine/tests/evidence/` (saves that back a fact in the facts file but no test).
- **Commits:** locally, per milestone or coherent step, only with all tests passing (two commits in this project's history broke that; don't repeat it). Stage explicit paths, never `git add -A`. Never push; the user pushes. No AI attribution in commit messages.
- Do not open the game's encrypted scripts (`.gde` in `vcb.pck`). Reading plain strings and constants in the executable is how the random generator was found, and that is the limit.

## The observation workflow
1. Write a text-art board in `engine/boards/<name>.txt` (by hand, or with a script in `engine/tools/board-generators/`). Every probed signal drives its own 3×3 LED block; `labels = on` draws the LED numbers on the board in Annotation ink.
2. `vcb gen engine/boards/<name>.txt -o saves/pending/<name>.vcb` writes the board, a blank checklist and a picture. The checklist never contains the engine's prediction.
3. The user steps the board in the game and writes the lit LED numbers per tick (`none`, `same` are allowed), or runs it and saves.
4. Import: move `<name>.vcb` to `engine/tests/boards/`, the filled checklist to `<name>.observed.md` with an "Observed by" line, and an after-save to `<name>.after.vcb`. The fixture tests pick them up by file name. Add the unit to `VERIFIED` in `src/ink.rs` in the same commit.

Prefer observation methods that need little manual stepping. The user's own idea of recording signals into VMem and saving with Persistent Memory gave 8,000 random bits in two runs; use that pattern when a lot of data is needed.

## Things that cost time once
- **Saving over an open file under Wine** leaves the new content in `<name>.vcb.tmp`. Look there when a save seems missing. "Save As" a new name is safe.
- **The game's autosaves** are in the Wine prefix: `~/Applications/Sikarugir/Steam.app/Contents/SharedSupport/prefix/drive_c/users/Sikarugir/AppData/Roaming/VirtualCircuitBoard/autosaves/`. `last_simulated.vcb` is the board as it was when the user last entered simulation mode, which shows edits the user made in the game without saving.
- **VMem needs at least 4 address bits and 4 content bits.** The game raises smaller values on load and the pins move.
- **Persistent Memory** is copied into the project when the user returns to edit mode, From..To inclusive, without word 0.
- **The game's tick counter at a Breakpoint pause is the engine's stop tick plus one.**
- **A tunnel entrance without a matching exit is a compile error in the game**, and tunnel scans run along the whole row, so unrelated test circuits on one row can interfere.
- **Two pulses on one trace need a tick between them**, or they merge into one rising edge.
- **Regenerating an observed board must give identical pixels** (a test checks it). Changing `textboard` label placement or a generator can break that.
- **`cargo test` leaves the debug binary current, not the release one.** Rebuild release before timing anything.
