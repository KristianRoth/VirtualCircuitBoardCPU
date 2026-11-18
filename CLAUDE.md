# VirtualCircuitBoardCPU

A 32-bit CPU built in the Steam game Virtual Circuit Board (VCB), and `engine/`: a Rust engine that runs VCB saves tick for tick like the game. The aim is to solve Advent of Code on the CPU, faster than the game allows.

## Read these first
- `AoC2025/` and `AoC2026/` are **separate git repositories nested in this folder** and git-ignored here (the user's choice, 2026-11-16). `AoC2025/` holds the CPU boards (`cpu-32bit-manual-start.vcb`, formerly `32-bit-alu-try.vcb`; `cpu-32bit-autorun-aoc1p1.vcb`, formerly `CPU.vcb`) and the programs in `days/day1/part1.vcbasm` and `part2.vcbasm`; see its README. Commit there with `git -C AoC2025 …`, never from this repo. The engine does not depend on them: its tests use their own copy of the CPU board in `engine/tests/boards/cpu-32bit-manual-start.vcb`.
- `specs/vcb-engine-v1.md`: the v1 design, the milestone table with status (Rollout), open questions, and the decision log. This is the plan being followed.
- `specs/vcb-game-facts.md`: everything observed about the game and the `.vcb` format, and every open question about the game. New facts about the game go here, with the date and the board that showed them.
- `specs/vcb-engine.md`: the framework spec (goals, CLI, stop rules). The v1 spec lists where it overrides this.
- `specs/vcb-engine-v2.md` and `specs/vcb-engine-v2-log.md`: the fast engine (specified 2026-11-18) and the log of every optimisation attempt. The Profile milestone is done (2026-11-18): the log holds the measurements, the noise (0.5%) and the ranked plan. The v2 engine exists (`engine/src/sim2.rs`, `--engine v2`, equality tests in `engine/tests/v2.rs`); attempts 1 and 3 are accepted (about 1.8 times v1), attempt 2 (compiler settings) was rejected. v1 is still the default. Every attempt gets the quick benchmark first (`--runs 1 --ticks 20000000`). **Next: attempt 4, at the user's word.** Still owed from Scaffolding: the event primitive and `--verify`. Benchmark: `python3 engine/tools/bench.py --old <bin> --new <bin>` from `engine/`, with nothing else heavy running. Rules it adds: v2 must show the same events as v1; a change to v1's model must reach v2 in the same commit; the first answer of a new puzzle program is confirmed with `--verify` (or `--engine v1`) before it is submitted.
- `specs/vcb-assembler.md`: the assembler's spec (written 2026-11-16, not built yet): milestones A1–A3, the verified/unverified/undefined gate, the probe boards. Its game questions are question 23 in the facts file.
- `engine/README.md`: build, commands, layout, how to run a program on the CPU.
- `manual/*.md`: the game's User Guide, transcribed. Git-ignored on purpose (the game's copyrighted text, public repo). Do not commit it.

## State (2026-11-16)
- Milestones M0–M7 of v1 are done. Every ink and VMem is verified against the game; Timer is "modelled".
- The CPU runs AoC day 1 part 1 to its Breakpoint at the game's tick with identical memory: `cargo test --release -- --ignored`, about 8 s, about 5.5 million ticks per second.
- `vcb run` also has `--verbose` (prints every change to a watched VMem word with its tick), added after M7.
- All three repositories (this one, `AoC2025`, `AoC2026`) have GitHub remotes under `KristianRoth` and were in sync with them at the end of the session. The user pushes; never push.
- **The assembler** (`specs/vcb-assembler.md`): milestone A1a is done. `engine/src/asm.rs` and `vcb asm` exist; day 1 part 1 assembles to the game's memory in every word. `vcb run` assembles the board's program before tick 0, as the game does (A1b; `--asm <file>` picks another program, `--no-asm` turns it off).
- A0 is done too: the game's assembled save of part 2 is a fixture (`cpu-aoc1p2.assembled.vcb`), and part 2 run by the engine ends with `count` = `0x16C7`. Both day 1 programs assemble from the user's own `.vcbasm` files.
- **The assembler is complete (A0–A4, 2026-11-17).** The probe saves are fixtures (`engine/tests/boards/asm-*`), and the assembler's rules are what the game showed: 32-bit signed arithmetic with C's precedence, addresses 1 to `0xFFFFF`, macros expanded in place. Nothing may follow a macro definition on its line (the game rejects a `;` after the closing brace). **A macro body may hold several statements** separated by `;`, with labels, and a placeholder may be glued to a name (`Starta?`); shown on 2026-11-18 by the game's compact sample computer.
- The second probe round is imported too (`asm-probe2`, `asm-crfield`, `asm-external-missing`). No assembler feature is unverified any more; question 23 has five small open cases (b, c, n, p, u).
- Nothing is waiting in `saves/pending/`.
- **The game's two 32-bit sample computers** (2026-11-18) are in `saves/pending/` (`32-bit-cpu.vcb`, `32-bit-cpu-compact.vcb`, and `32-bit-cpu-compact-assembled.vcb`, the game's save after assembling). They are the game's own boards: never commit them. Edited copies that run the primes program without input are in `saves/bench/` and are programs 9 and 10 of the v2 benchmark (`engine/tools/bench.py`). Running them needed two models, both in the facts file and not yet compared with the game tick for tick: a VMem pin of size 1×n is one pin over n pixels downwards (question 4), and a bus at a tunnel is matched by its own colour (question 12).

## The CPU boards and their fixes (2026-11-17, 2026-11-18)
- The four CPU boards in `AoC2025/` carry a 50-pixel fix (x 1120–1129, y 215–224) that makes `shf | n` shift A left by n, and a 24-pixel fix beside it (x 1130–1133) that makes `xor` and `neg` (a bitwise NOT) work. Both were found and checked in the engine only; the user has not yet run a patched board in the game. `AoC2025/findings/cpu-instructions.md` describes the CPU's instructions and the fix; `AoC2025/RULES.md` holds the rules for solving puzzles on the CPU.
- **Every copy of the CPU is patched, the engine's fixtures too** (`engine/tests/boards/cpu-*.vcb`, at the user's request on 2026-11-18). Those fixtures' memory, tick counts and assembly text are still what the game produced, but with the board as it was before the fixes: the 74 fix pixels were drawn into the saved files afterwards. In the engine the patched and unpatched boards run the fixture programs identically, tick for tick. The game has not yet run a patched board; when it has, these fixtures can be replaced by fresh saves.
- `saves/pending/cpu-shift-test.vcb` and `cpu-shift-fix-test.vcb` are shift test boards for the game (both carry both fixes now); a `non` before each shift pauses the CPU there.
- `engine/examples/trace.rs` inspects a board (text art, what an entity connects to, a bus and its taps, signal changes during a run).

## Loose ends
- **No licence** has been chosen for `engine/`, and it is already on GitHub. Raise it with the user.
- **The root `README.md`** was extended with a short description; the CPU's instruction table is also in `AoC2025/README.md`. The user may want to rewrite it.
- **Leftover files the user may delete:** `32-bit-alu-try.vcb.tmp` in the root, and in `saves/` two 4 MiB `.vcbmem` files and two `.vcb.tmp` files. All are git-ignored and nothing uses them.
- **Unobserved game behaviour** (none of it used by the CPU): a non-zero word 0 preloaded in a file, VMem pin sizes other than 1×1, a Read or Write pixel touching a tunnel, bus or mesh, Random draw order in mixed cases. See the open questions in the facts file.

## What could come next (the user decides)
- AoC day 1 part 2 (`AoC2025/days/day1/part2.vcbasm`), and AoC 2026 in `AoC2026/`.
- The v2 engine: Scaffolding and then Stage 1 of the ranked plan in `specs/vcb-engine-v2-log.md`, once the user has approved or reordered it.
- Measuring the game's own speed on the Windows PC, to compare with the engine.

## Building
`cargo` is not on the PATH and the default macOS 27 SDK breaks linking:
```
export PATH="/opt/homebrew/opt/rustup/bin:$PATH" SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX26.5.sdk
cd engine && cargo test
```
Use `./target/release/vcb` for long runs; the debug build is far slower.

## Rules of this project
- **The game is the ground truth.** If the engine disagrees with an observation, the engine is wrong until the user has re-observed that case.
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
