# Where v2.1 ended (2026-11-20)

The user closed v2.1 here. This file is the place to start for any further work on the engine's speed: where it stands, what was found, what is left to try, what not to try again, and how to run every tool. The detail behind each line is in `vcb-engine-v2-log.md` (every attempt with its numbers), `vcb-engine-v2.1.md` (the reasoning as it went), and `vcb-engine-profile-2026-11-20.md` (the profile and the processor's counters).

## 1. Where it stands

Day 4 part 2 (1,339 million ticks) to its Breakpoint: **6.4 s** with the profile-guided build. v2 at the end of stage 1 took 14.2 s, v1 230.5 s, and the game, at the user's figure of 5 million ticks per second, would take about 268 s.

| Program | Ticks | Game (5 M ticks/s) | v1 | v2, end of stage 1 | v2.1 now | Now against the game | against v1 | against v2 |
|---|---|---|---|---|---|---|---|---|
| day1/part1 | 43.1 M | 8.6 s | 7.0 s | 0.6 s | 0.28 s | 31 | 25 | 2.3 |
| day1/part2 | 70.3 M | 14.1 s | 11.7 s | 1.1 s | 0.48 s | 29 | 24 | 2.3 |
| day2/part1 | 43.5 M | 8.7 s | 7.3 s | 0.3 s | 0.15 s | 60 | 50 | 2.0 |
| day2/part2 | 75.8 M | 15.2 s | 12.9 s | 0.4 s | 0.19 s | 79 | 68 | 2.0 |
| day3/part1 | 170.9 M | 34 s | 27.8 s | 1.1 s | 0.50 s | 68 | 55 | 2.2 |
| day3/part2 | 419.7 M | 84 s | 65.9 s | 3.3 s | 1.5 s | 56 | 44 | 2.2 |
| day4/part1 | 481.3 M | 96 s | 83.0 s | 4.9 s | 2.3 s | 42 | 36 | 2.2 |
| day4/part2 | 1,339.4 M | 268 s | 230.5 s | 14.2 s | 6.5 s | 41 | 36 | 2.2 |
| sample (first 100 M) | 100 M | 20 s | 17.3 s | 0.2 s | 0.11 s | 179 | 154 | 1.8 |
| compact sample (first 100 M) | 100 M | 20 s | 7.4 s | 1.9 s | 0.47 s | 42 | 16 | 4.0 |
| **Geometric mean** | | | | | | **54** | **42** | **2.3** |

Speeds in million ticks per second, same order: v1 6.2, 6.0, 6.0, 5.9, 6.2, 6.4, 5.8, 5.8, 5.8, 13.5; v2 at the end of stage 1 69, 64, 148, 197, 153, 129, 98, 94, 495, 53; v2.1 now 157, 146, 298, 397, 339, 280, 211, 207, 893, 212.

How these numbers were got:
- v1 and v2: measured, from the stage 1 table in the log (attempt 15, plain builds).
- v2.1: **derived, not measured in one run**: the last full-length benchmark with absolute speeds (attempt 32) times the measured full-length gains of attempts 33 and 34, per program. Plain builds. The one direct check: day 4 part 2 comes to 6.5 s this way and measures 6.4 s with the profile-guided build.
- The game: the user's figure (2026-11-20), applied to every program alike. On which machine and board it was measured was not said, so it is not in `vcb-game-facts.md`.

## 2. What v2.1 added to the engine

Stage 1 ended with replay, the clock schedule, liveness and the counting tick (`vcb-engine-v2.1.md`, "What v2 does"). On top of that:

| Attempt | What | Gain |
|---|---|---|
| 16 | The rare parts of a tick (VMem, Random) as cold functions | 1.03 |
| 18 | A latch keeps a toggle flag; no just-risen marks | 1.05 |
| 19 | The packed state kept up to date at every change (no packing at events) | 1.11 |
| 20 | One 8-byte record per entity | 1.03 |
| 22 | The replay table emptied at its limit; kept between runs with `--replay-cache` | a second run of day 4 part 2 in 1.3 s |
| 23 | Replay on boards with a simulated Timer | the compact sample 2.1 |
| 24 | Profile-guided build (`tools/build-pgo.sh`) | 1.12 |
| 27 | **Records**: the part of a step that is proven the same for any data is applied from a record, only the variable part is simulated (`src/sim2/split.rs`) | 1.20 to the Breakpoints |
| 28 | The proof on a worker thread | 1.13 quick, 1.04 to the Breakpoints |
| 29 | Five trims around a step (the hash's lanes, the record check, one context lookup, hot paths not marked cold, the first flips reused) | 1.05 |
| 30 | Five more (the queue push without growth, the last word in a replay edge, the channel poll, the net change as a mask, bounds checks) | 1.04 |
| 32 | Records on boards with a simulated Timer | the compact sample 1.05 to 1.08 |
| 33 | `propagate_split` tuned (the record lists its variable readers) | 1.02 |
| 34 | **The wake and the push in `feed` without branches** | 1.08 |

Rejected, with the code on local tags `v2-attempt-NN-…-rejected`: 17 (replay keyed on the word's content bits), 21 (renumbering), 25 (watched inputs), 26 (one-input gates without a count), 28's bit splitting and extra workers, 30's item 4 (clock positions without divisions), 31 (six instruction-level changes to `propagate`), 32's two stronger Timer forms, 34's record layout, reader lists by kind, and flat reader loop.

## 3. What was found

The facts that should steer further work. Each was measured; where and how is in the files named.

**About where the time is** (profile of 2026-11-20, at attempt 32; the shares have shifted a little since):
- Replay crosses about two thirds of the ticks and costs almost nothing: 45 cycles per replayed step. **The size of the replay table (1.3 GB on day 4 part 2) is not a cost.**
- Of the steps replay misses, 93 to 96% run with a record. Nine flips in ten are made in such a step.
- The hottest function is `propagate_split` (43 to 45% then), then `step_real` (18%), ending and starting a step with a record (8% and 6%), replay's bookkeeping (about 11% in many small pieces).
- A step with a record: about 107 flips, 18 ticks, 36,000 instructions, of which the ticks are 78%, ending it 10%, the table's bookkeeping 6%, starting it 3%, putting a replayed state back 3%. A whole step: 293 flips, 37 ticks, 54,000 instructions.

**About what costs** (the processor's counters, attempt 34):
- **A mispredicted branch costs about 50 cycles in this engine.** Removing 11.8 of 36.9 per step, at the price of 8% more instructions, took 10% off the cycles.
- **A first-cache miss costs nearly nothing.** Removing 36 of 121 per step (and a third of the TLB misses) changed nothing. The records do cause those misses (120 per step with them, 27 without), and it does not matter.
- Every mispredicted branch is an ordinary conditional one; about one instruction in six is a conditional branch and about 2% of them are mispredicted. **After attempt 34 about 25 are mispredicted per step: by the price above, over a third of the cycles.**
- Instruction counts mislead in the reader loop: the branch-free `feed` is 31 instructions per reader where it was 16 to 22, and faster. Keeping more in registers lost twice (attempts 31 and 33); reloads from the first cache are free on this machine.

**About the circuit and the records:**
- Two thirds of the flips in a step can be proven the same for any data, given what is known when the step starts (63 to 68% on the user's CPU and the samples). That is what the records use.
- A simulated tick walks about eight reader lists, most with none, one or two readers; a few long lists (five or more, about fourteen readers) carry three quarters of the tellings.
- Most reader lists hold one kind of gate (82% on the user's CPU after sorting); none more than two.
- One-input gates are 5% of the components and 11 to 15% of the flips on the user's CPU; only 5% of flips could go by collapsing chains.
- The compact sample is another picture: more of each step depends on data, and its Timer flips inside most steps, so a record covers less (a quarter fewer flips, against more than half on the user's CPU).

**About method:**
- The quick benchmark has about 1% noise per program, and a single run can be 2 to 3% off (one run of attempt 34 gave 1.061 where two others gave 1.084 and 1.091). Changes under about 2% need three runs and the full-length benchmark.
- Mispredictions and instruction counts from the counters are repeatable to a fraction of a percent, and other load does not disturb them. They are a better judge of a change in the tick than the clock.
- Estimates made with hindsight over a whole run are too good: bit splitting was estimated to halve the remaining work and gave 13 to 19% fewer flips and no speed.
- Parallel work in separate worktrees works if nobody times anything while others build; the timings are then made one after another.

## 4. What is left to try

In the order I would take them. Sizes are guesses unless a measurement is named.

**A. The remaining mispredicted branches (the lever the counters point at).** About 25 per step are left. Candidates, each to be judged by `counters` first and the clock second:

1. **Evaluating the queued gates in `step_real` without branches.** Each tick evaluates 6 to 8 gates with a `match` on the kind and an `if next != on { flips.push }`. The same treatment as attempt 34: the output computed as bits, the gate always written to the flips list, the length advanced by whether it flips. `evaluate` and the loop are in `sim2.rs`. Not tried.
2. **The test whether a reader is fixed or variable, in `propagate_split`.** A coin toss per reader. With `feed` branch-free it could become a mask on the wake and the push (count always; toggle, queued and push only if variable). A similar variant lost 6% in attempt 33 ("M"), but that was before the branch-free `feed`, and it was judged by the clock only.
3. **Whether a trace changed** (`now == on` in `propagate` and `trace_split`): the readers' loop could run with a zero step, or the trace's readers be told through a branch-free path. Not tried; the lists are short, so it may not pay.
4. **The sample CPU's loss in attempt 34** (2 to 6%): most of its readers are never woken, so it pays three stores per reader for little. A form that stores only when woken without branching back in is worth a look if that board matters.
5. To see which of these is worth most before building: `counters` with `parts` and `events=branches` together gives mispredictions per part of a step; finer than that needs a hook around the evaluate loop (the `--cfg parts` hooks are in `advance` and `leave_split` only).

**B. Records, further.**

6. *(Closed, attempt 35: records kept in a file beside `--replay-cache`. Built after v2.1 ended, and no gain in any case, because a run has its records within its first few thousand steps anyway; see section 5.)*
7. **The step's end.** `leave_split` is 10% of a step's instructions and the table's bookkeeping after it 6%. Attempts 29 and 30 trimmed both; what is left has not been taken apart further than the counters' parts.
8. **A start check on only the signals the proof relied on.** When no record fits, the closest one would have been right a third to two thirds of the time. Sound if the proof notes which known signals it used. Ceiling measured: under 1% on the user's CPU, perhaps 2% on the sample. Low.
9. **Known states inside a step** (look up the state when the circuit comes to rest, not only at events). 7 to 19% of the ticks in missed steps come after a state seen before; many of those are idle ticks already jumped, and the estimate leaves out the word a step loads. Low to middling; needs a better estimate first.

**C. Boards that are behind.**

10. **The compact sample** (a Timer): `propagate` in whole ticks is a third of its time and `restore` 10%, four times its share elsewhere. The `restore` share has not been looked at at all.
11. **Boards with Random ink** have no replay and so no records. Nothing is planned; each evaluation of a Random uses up a draw, which is why.

**D. Large and uncertain.**

12. **Compiling**: a routine per component or per context in place of walking lists (the second research pass's B1 to B3). With the reader loop now one branch-free block the case for it is weaker than it was.

**E. Small things outside the benchmark.**

13. `--verbose` writes each line through an unbuffered, locked stdout; it probably sets the speed of a verbose run.
14. Clippy's performance lints have never been run on the crate (`cargo clippy --release -- -W clippy::perf`).
15. The game's speed per board, measured, to replace the single figure in section 1; with machine and board it belongs in `vcb-game-facts.md`.
16. Loose ends in the code, each marked `shortcut:` there: no more proofs past 50,000 records and records are never freed (`split.rs`); the worker gives up on a step of more than 100,000 ticks.

## 5. What not to try again

Measured and closed. The log has the numbers.

- **In the reader loop:** fewer notifications (watched inputs, 25), one-input gates without a count (26), fewer instructions or loads (31), keeping values in registers (31, 33), reader lists sorted by kind or a loop per kind (34), one flat loop per tick (34).
- **Cache and memory:** renumbering entities (21), a record in one piece (34), anything aimed at the replay table's size, another allocator, prefetching.
- **Replay keyed on less:** the word's content bits (17), read sets and masks on the key (v2.1 steps 1 and 8, group A's A3 and A4).
- **Records:** keeping them between runs in a file (35: nothing to save, and another program's learning state makes proofs worse); contexts split on single bits, in two forms (28); more than one worker thread (28); the Timers' position in the context, and a new stretch after each Timer flip (32); guessing a record that does not fit.
- **Other threads:** running the next step ahead from a guessed state (the best guess is right 16 to 28% of the time); hashing or table work on another thread (the hand-over costs more than the work).
- **SIMD and bit-parallel simulation** across gates, time or data (the third research pass).
- **Netlist clean-ups:** duplicates and constants (12 and 30 of 1,858 components), delay lines for chains of one-input gates.
- **Part-level skipping** (12), **chaining replay steps** (9).

## 6. How to run everything

All from `engine/`, with:

```
export PATH="/opt/homebrew/opt/rustup/bin:$PATH" SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX26.5.sdk
```

`<board>` is a `.vcb` file; `<program>` a `.vcbasm` file, or `-` for the program in the board. The user's CPU: `../AoC2025/cpu-32bit-autorun-aoc1p1.vcb` with `../AoC2025/days/dayN/partM.vcbasm`. The samples: `../saves/bench/32-bit-cpu-primes.vcb` and `../saves/bench/32-bit-cpu-compact-primes.vcb` with `-` (the compact one needs `--timer-ticks 1000` in `vcb run`; the examples set it themselves).

### Building

| What | Command | When |
|---|---|---|
| The engine for running | `sh tools/build-pgo.sh` | About 70 s. The fastest build; a plain `cargo build --release` replaces it. |
| A plain build | `cargo build --release` | For comparing two versions in the benchmark, about 10 s. |
| The examples | `cargo build --release --examples` | Or `--example <name>` for one. |
| Counters on, functions apart | `RUSTFLAGS="--cfg profiling -C target-cpu=native" cargo build --release --example <name> --target-dir target/profiling` | For `profile2`, and for `split_time` to print ticks and flips. |
| Functions apart, no counters | `RUSTFLAGS="--cfg separate -C target-cpu=native" cargo build --release --target-dir target/separate` | For sampling and for reading machine code. |
| Hooks at the parts of a step | `RUSTFLAGS="--cfg parts -C target-cpu=native" cargo build --release --example counters --target-dir target/parts` | For `counters … parts`. |

`cargo test` leaves the debug binary current, not the release one: rebuild before timing anything.

### Running a program

```
target/release/vcb run <board> --asm <program> --ticks 2000000000
```

Options that matter for speed: `--engine v1` (the reference), `--verify` (both engines, every event compared; about 200 s for day 4 part 2), `--replay-cache <file>` (keeps the replay table between runs), `--replay-memory <MiB>`, `--timer-ticks <n>`. `engine/README.md` has the whole list.

### Tests

| Command | What | Time |
|---|---|---|
| `cargo test` | Everything in a debug build, with the debug assertions on every unchecked index | 1 min |
| `cargo test --release` | The same in a release build | 10 s |
| `cargo test --release -- --ignored` | Whole programs against v1, with stops inside steps | 4 to 5 min |
| `target/release/examples/split_check <board> <program> [ticks] [stride]` | v2 with records against v2 without, at every event and every `stride` ticks, counts checked. Must print "the same". | seconds |

For a change to the tick or the records: all four, `split_check` on day 4 part 2 with a stride (211 has been the habit), on day 1 part 2 without, and on both samples; and day 4 part 2 with `--verify` before believing a first answer.

### The benchmark

```
python3 tools/bench.py --old <binary> --new <binary> --old-engine v2 --new-engine v2 --runs 3 --ticks 20000000
python3 tools/bench.py --old <binary> --new <binary> --old-engine v2 --new-engine v2 --runs 2 --full
```

The first is the quick one (about 90 s), the second runs every program to its Breakpoint (about 3 min). Ten programs; the last line is the score, the geometric mean of new over old. Copy each build's `target/release/vcb` aside under its own name first. Nothing else heavy may run. The bar for keeping a change is 1.015 on the quick benchmark; near it, run it twice and run the full one.

### Profiling by sampling

```
target/separate/release/vcb run <board> --asm <program> --ticks 2000000000 & sample $! 9 1 -file out.txt
python3 tools/sample_summary.py out.txt
```

`sample` is macOS's own. The summary gives, per thread, the share of samples whose top of stack is in each function. The profile-guided build inlines nearly everything into the run loop, which is why the `separate` build is sampled. Machine code of one function: `nm target/separate/release/vcb | grep <name>`, then `objdump -d --no-show-raw-insn --disassemble-symbols=<symbol> target/separate/release/vcb`. The two annotated listings are `vcb-engine-propagate-asm.s` (before attempt 34) and `vcb-engine-propagate-split-asm.s` (before attempt 33): both are out of date as code and still show how to read it.

### The processor's counters (cache misses, mispredicted branches)

They need root, so the user runs them. From the repository root:

```
sudo sh engine/tools/counters-runs.sh > engine/target/counters-out.txt 2>&1
```

That makes seven runs on day 4 part 2 (as usual, without records, with a small table, both; the branch events with and without records; the parts of a step) in about fifteen seconds. It needs `target/release/examples/counters` and, for the last run, `target/parts/release/examples/counters` (see Building). Two optional arguments name other binaries: the plain one and the parts one.

By hand:

```
sudo target/release/examples/counters <board> <program> [ticks] [every] [nosplit] [table=<MiB>] [events=branches] [parts] [dry]
```

- `every`: read the counters after every so many steps (1 unless given). With 1 the report is per kind of step (replayed, with a record, whole) but each reading disturbs the caches of the step after it (misses come out a sixth too high, TLB misses twice); with 1000 everything is one kind and nothing is disturbed. Instructions and mispredictions are right either way.
- `nosplit`: no records. `table=<MiB>`: a limit on the replay table.
- `events=branches`: conditional and indirect branches executed and mispredicted, in place of the cache events. The processor counts at most six of these events at once; the tool drops what does not fit and says so.
- `parts`: per part of a step (needs the `--cfg parts` build).
- `dry`: no root needed; the clock stands in for every event, to try the tool.

To compare variants: build `counters` in each variant's tree, and run each with `300000000 1000`; `branches mispredicted` and `instructions` per step are the lines to compare.

### The examples

Each prints its own usage at the top of its source. `cargo run --release --example <name> -- <arguments>`, or the built binary under `target/release/examples/`.

| Example | What it tells | Arguments |
|---|---|---|
| `split_check` | v2 with records equals v2 without | `<board> <program> [ticks] [stride]` |
| `split_time` | Time with and without records, and the records' counters (in a profiling build also ticks and flips) | `<board> <program> [ticks]` |
| `counters` | The processor's counters (above) | |
| `profile2` | v2's counters: ticks replayed, idle, simulated; flips; feeds by kind (profiling build) | `<board> <program> <ticks> [noreplay]` |
| `profile` | The same kind of measurement for v1 | `<board> <program> activity\|repeats <ticks>` |
| `replay_stats` | What replay did: states, memory, ticks crossed | `<board> <program> <ticks> [limit in MiB] [flush]` |
| `replay_estimate` | How often replay would hit, on v1 | `<board> <program> <ticks>` |
| `live` | How much of a board v2 simulates | `<board>` |
| `netlist_stats` | Entities by kind, duplicates, constants, one-input gates | `<board>` |
| `one_input` | The share of flips made by one-input gates and chains of them | `[ticks] [board] [program]` |
| `autonomous`, `clock_fan` | The part of a board the clock drives by itself; how far the clock reaches | `<board>` |
| `heatmap` | A picture of how often each entity flips | `[ticks] [board] [program] [output prefix]` |
| `parts`, `parts_tree`, `parts_estimate` | Sparsely connected parts of a board (the rejected attempt 12): pictures, a tree as JSON, what skipping them could save | see each |
| `ternary_estimate` | How much of a step can be proven the same for any data (what the records rest on) | `<board> <program> [ticks] [sample]` |
| `split_estimate` | The same with contexts split on single bits (rejected in attempt 28) | `<board> <program> [ticks] [sample]` |
| `group_a_estimate` | Context masks on the replay key, the net change of a step, known states inside a step | `<board> <program> [ticks]` |
| `readset_estimate`, `control_estimate` | Replay by what a step reads; control replayed and data simulated (both closed) | `<board> <program> [ticks]` |
| `ahead_estimate` | How well the next step's start can be guessed (closed) | `<board> <program> [ticks]` |
| `trace`, `diff_boards` | Inspecting a board's wiring and signals; the pixels that differ between two boards | see each |

### Working on several ideas at once

`git worktree add -b <branch> .claude/worktrees/<name> HEAD` gives each idea its own tree and its own `engine/target`. The nested `AoC2025/` and the `saves/` folder are not in a worktree: give the checker absolute paths to the boards, and run `tools/bench.py` from the main checkout's `engine/` with the worktrees' binaries as `--old` and `--new`. Build and test in parallel; time one after another with nothing else running. Merge with `git cherry-pick`, then `git worktree remove` and delete the branch; put what is rejected on a tag first.

## 7. The state of the repository

- Everything is committed on `main`. The history since the last push was squashed on 2026-11-20 into eleven commits, one per attempt or pair of attempts (the user pushes; never push). Three local backup branches hold the history as it was before each clean-up (`backup/before-squash-2026-11-19`, `…-19b`, `…-2026-11-20`).
- Rejected code is on fourteen local tags, `v2-attempt-NN-…-rejected`.
- `engine/target/release/vcb` is the profile-guided build of the last commit.
- `darwin-kperf` is a dev-dependency (only `examples/counters.rs` uses it); the `vcb` binary does not contain it.
- Commit hashes quoted in the log and in the agents' reports are from before the history was squashed; the tags still point at the old commits.
- `engine/README.md`'s speed table is still stage 1's; section 1 above is the current one.
