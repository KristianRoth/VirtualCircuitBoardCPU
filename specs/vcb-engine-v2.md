# VCB engine v2 (the fast engine)

## Problem & goal
The engine runs the CPU at about 5 to 6.6 million ticks per second. That was enough for the first puzzles, but Advent of Code day 4 part 2 takes 1.34 billion ticks: 4.6 minutes. Later puzzles will be longer.

The goal is a second engine, v2, that runs the CPU boards **as fast as possible**, runs **any** board correctly, and shows **the same observable events as v1**. There is no target number. The work proceeds in measured attempts, and the user decides after each stage whether to go on.

The end product is two things: the engine, and a log of every optimisation that was tried, what it gained, and whether it was kept (`specs/vcb-engine-v2-log.md`).

Related documents:
- `vcb-engine.md` (the framework spec): the CLI, stop rules and outputs. This spec replaces what it says about v2; the lines are listed under Rollout.
- `vcb-engine-v1.md`: the reference engine. v1 is what is compared with the game. v2 is only ever compared with v1.
- `vcb-game-facts.md` (the facts file): what is known about the game. Its question 22 is the order in which Random inks draw.
- `vcb-engine-v2-log.md` (the log): the profile, the ranked plan, and one entry per attempt. The log is the one home of every measurement; other documents copy from it.

### Terms
- **v1:** the reference engine in `engine/src/sim.rs`. It computes every tick with a list of the components whose inputs changed (the dirty list), and is verified against the game.
- **VMem:** the game's memory component. The circuit reaches it through address pins and content pins. After an address change VMem is locked for one tick.
- **Breakpoint:** an ink that pauses the game when it turns on; the CPU's programs end on one.
- **Event:** something a user can observe during a run. Defined under "What must be equal".
- **Attempt:** one optimisation idea, implemented, measured and judged. It has one entry in the log.
- **Stage:** a group of attempts. A stage ends with full benchmark runs and a stop for the user's decision.
- **The benchmark set:** the real-input Advent of Code programs in `AoC2025/days/` (not the `-example` ones), each run on `AoC2025/cpu-32bit-autorun-aoc1p1.vcb` with `--asm`. Today that is eight: days 1 to 4, parts 1 and 2. A ninth was added at the user's request on 2026-11-18, before any attempt: the game's 32-bit sample computer, a different CPU design, with its program edited to run its primes program without input (`saves/bench/32-bit-cpu-primes.vcb`; the game's own board, so git-ignored and not in the repository). A tenth the same day: the compact version of that sample (`saves/bench/32-bit-cpu-compact-primes.vcb`, run with `--timer-ticks 1000`). Neither has a Breakpoint; both always run to the tick limit. The set is fixed for the length of a stage; a new day joins at a stage boundary, and the baseline is measured again.
- **Measured run:** a benchmark program run to its Breakpoint or to 100 million ticks, whichever comes first.
- **Milestones** are named Profile, Scaffolding, Stage 1, Stage 2 and so on (not V0, V1: that reads like the engines).

## Users & roles
- **The user** decides after the profile and after each stage whether and how to go on.
- **The engine author (Claude)** profiles, plans, runs the attempts, applies the acceptance rule, and keeps the log.

## Scope
### In scope
- A second engine, chosen with `--engine`.
- Every board v1 runs, including Random and Timer inks.
- Natively in v2: `--ticks`, Breakpoint, `--no-breakpoint`, `--break-addr`, `--quiescent`, Ctrl-C, `--watch`, `--dump-vmem`, `--stats`, `--verbose`.
- Automatic fallback to v1 for `--probe` and `--render`, which show internal signals.
- `--verify`: run v2, then v1, and compare.
- The equality tests, the benchmark script, the log.

### Out of scope
- Any change to how v1 computes a tick. (The shared trait and the command line change; see Design.)
- Using several cores inside one run. (Several `vcb` processes can run side by side.)
- A viewer, VInput, VDisplay.
- Making the CPU's programs faster.
- A fixed speed target.

## What must be equal
v2 and v1 must show the same **events**, the same **stop** and the same **final memory**. v2's internal signals need not match v1's: v2 may drop, merge or never compute them.

| Observable | Requirement |
|---|---|
| The events of a run, in order | Identical. An event is one of: VMem loads from an address (tick, address, the word loaded); VMem is written (tick, address, the word, whether the word changed); a Breakpoint turns on (tick). |
| Stop tick and stop reasons | Identical, for every stop: `--ticks`, Breakpoint, `--break-addr` (fetch, write, any), `--quiescent`. Also when several hold on one tick: the same reasons in the same order. |
| VMem at the stop | Identical in every word. |
| Standard output | Byte for byte the same as v1's for the same command, apart from nothing: `--watch`, `--verbose` and the stop line are the same text. Times in `--stats` (standard error) differ. |
| Random ink | The same draws in the same order, so a seeded run gives the same events. |
| Errors | A board or option v1 refuses is refused by v2 with the same message. v2 is built from v1's checked netlist, so this holds by construction. |

What need not be equal:
- **Internal signals,** at any tick. `state()` is not part of v2's contract.
- **Ctrl-C.** v1's stop tick depends on the wall clock too. v2 stops within two seconds at a tick it has a true state for, reports that tick, and writes the outputs for that tick. They equal what v1 gives with `--ticks` set to that tick.
- **Progress lines** of `--stats` while running.

`--probe` and `--render` need internal signals at every tick. A run with either is done by v1, and `vcb run` prints one line on standard error, after the board and options are accepted: `using engine v1: --probe needs every tick` (naming the first such option given). With an explicit `--engine v1` the line is not printed.

## Design
### The contract between the engine and the run loop
v1's trait is per tick: `run_until` calls `step()`, a callback and the stop checks after every tick. v2 cannot be fast behind that. The contract is therefore built on events:

- v2 offers one primitive: **advance to the next event, or to tick N, whichever comes first.** It returns the event, if any.
- The run loop is shared: it asks the engine to advance, applies the stop rules, and prints what the options ask for. The stop rules exist once.
- Between two events nothing observable happens, so only three stops can fire there, and none needs the circuit: `--ticks` (a tick number), `--quiescent` (the tick of the last write that changed a word, plus n) and Ctrl-C (checked at least every half second of work).
- v1 implements the same primitive by stepping tick by tick until an event occurs. Its own per-tick `run_until` and callback stay as they are for `--probe`, `--render`, the fixture tests and `examples/`.
- **Stopping inside a skipped stretch.** When the tick to stop at lies inside a stretch v2 would skip or replay, v2 does not skip it: it runs that stretch tick by tick. v2 therefore always keeps a tick-exact core that can run from any state it holds.
- **`--verbose`** prints write events, so it is native.
- The change to the trait and to `run()` in `main.rs` is its own commit, made before any v2 code, with every existing test passing unchanged. v2 consumes `netlist::Components`; it never reads the board file itself.

### What a remembered state must contain
For the families that replay or jump (the last two in the table below), "the same state" means everything v1's future depends on, not only which signals are on:

- every trace and component that is kept, on or off;
- which traces rose in the last tick, and the pending work with its order;
- the VMem lock, a pending load, and the address the pins hold;
- the position within the clock interval and within the timer interval;
- the Random generator's position;
- every word VMem delivered during the stretch: either part of the key, or the stretch ends at each load.

A replay is used only after the full recorded state has been compared; a hash only finds candidates. As built (attempt 8 in the log): the pending work is stored as the components that flip in the next tick, which is what it amounts to; every VMem load ends a step, so delivered words select the successor and are not part of the state; replay is off on boards with Random ink. On a board with a simulated Timer (since attempt 23) the timers' position in their interval is not part of a state, and a step is recorded and replayed only if no timer flips in it. The profile counts repeats on this full key.

### The engine optimises the simulator, not a board
Decided by the user on 2026-11-19. The goal is a faster simulator for any board, not a faster run of the user's CPU.
- **Nothing in the engine may come from knowing a particular board.** No coordinates, no block positions marked by hand, no constants tuned on one board, no rule that exists because of how the user's CPU is built. Whatever the engine uses, it finds by itself in the netlist of the board it is given, or learns while running it.
- A threshold or constant in the engine needs a reason that holds for boards in general. If it was chosen by trying values on the benchmark boards, the log entry says so and shows it on boards of more than one design.
- The benchmark set must keep boards of more than one design (today the user's CPU and the game's two sample computers). An attempt that gains only on one design is reported as such, and its entry says why it is still general.
- Looking at a board to understand where time goes (the heat map, the parts pictures) is fine. Feeding what was seen back into the engine as board-specific knowledge is not.

### Limits
- **Memory:** all of v2's tables and caches together stay under 2 GiB by default (`--replay-memory` sets it). When the limit is reached the replay table is emptied and learnt again (since v2.1 step 5; before that it stopped learning). v2 keeps running correctly either way.
- **Random ink:** stays native. An optimisation that cannot keep v1's draw order switches itself off on boards with Random ink; such a board then runs on v2's tick-exact core.
- **A later change to v1's model** (a new game fact) must reach v2 in the same commit. If v2 cannot follow, that commit makes v2 decline boards with the affected ink: `vcb run` uses v1 and prints the notice line with the reason. This is the only board-level fallback.
- **Code:** speed comes first. Unsafe code, generated machine code and new dependencies are all allowed without asking. The equality tests carry the proof, and they are also run once per stage with debug checks switched on.

## Behavior
### Choosing the engine
- Until Stage 1 is finished, v1 is the default and v2 runs with `--engine v2`. (Stage 1 was closed on 2026-11-19: v2 is the default now.) Stage 2 starts from `vcb-engine-v2.1.md`: what v2 does, what stage 1 found, the research findings of 2026-11-19, every open idea and the proposed order to try them.
- From the last commit of Stage 1 on, v2 is the default and `--engine v1` forces the reference.
- Every fallback is decided before tick 0 and announced with the notice line. The `--stats` line names the engine that ran (`engine: v2`).
- `--verify` runs the command on v2, then on v1, and fails loudly at the first difference in events, stop tick, reasons or final VMem. **The first answer of a new puzzle program is confirmed with `--verify` before it is submitted;** the program then joins the benchmark set.
- The fixture tests that compare the engine with the game keep using v1.

### The process
1. **Profile.** Measure v1 on the benchmark set and write the first section of the log:
   - how many components are evaluated per tick, and what fraction of all components that is;
   - where the time goes: evaluating components, bookkeeping of the dirty list, VMem, the per-tick callback and stop checks;
   - how often the circuit's full state (see Design) repeats, and how long a stretch is the same when it does;
   - whether the first 100 million ticks of the long programs resemble the rest;
   - **the noise:** the same build measured against itself, as the rule below measures two builds;
   - a ranked list of attempts with the gain and the effort expected from each, grouped into stages. An expensive family (compiled code, word-wide slices, lookup tables, replay) enters a stage only with a number from the profile that justifies it.

   Then stop. The user approves or reorders the list.
2. **Scaffolding.** The trait change; v2 as a copy of v1's core in new files; `--engine v2`; the equality tests; the benchmark script. This is attempt 0 in the log: v2 within the noise of v1, all tests passing.
3. **Attempts.** Within a stage, one at a time:
   1. Write the entry's first half in the log: the idea, the expected gain and why.
   2. Implement it in v2's files only. (A compiler setting applies to both engines; the log then gives the gain for both, and v1 is re-baselined.)
   3. Run the equality tests. An attempt that fails them is fixed or rejected; it is never measured as a success.
   4. Measure, as the rule says.
   5. Write the verdict.
   6. **Accepted:** commit, with its log entry. **Rejected:** commit the code on a local tag (`v2-attempt-07-rejected`), restore the engine's files to the last accepted commit, and commit the log entry, which names the tag.
4. **End of a stage.**
   1. Run every benchmark program to its Breakpoint with `--verify`. The log records the `AoC2025` commit.
   2. Re-measure each attempt accepted in the stage by switching it off or reverting it; one that no longer earns its place is removed and its entry updated.
   3. Put the table in the log (ticks, v1 time, v2 time, the ratio, peak memory of the longest run) and copy it to `engine/README.md` with the stage and date.
   4. Stop for the user's decision.

   If a full run differs: the default goes back to v1 in the same commit, the difference is found by bisecting the stage's commits, the faulty attempt and what depends on it are reverted with their entries corrected, and a test that reproduces the difference joins the equality tests.

### The acceptance rule
- **What is measured.** The last accepted build and the new build are run alternately in one session (old, new, old, new, …) on every program of the benchmark set, as measured runs, five times each. Per program the best of five counts. Start-up time (everything before tick 0) is reported separately and is not part of the speed.
- **The score** is the geometric mean, over the programs, of new speed divided by old speed.
- **Accepted** when all equality tests pass and the score is at least **1.015** and exceeds 1 by more than twice the noise the profile measured (0.5%, so 1%). The bar was 1.03 through stage 1; the user lowered it on 2026-11-19, in v2.1: the engine is optimised far enough that single steps of 3% have become rare, and 1.5% is still clear of the noise when measured over three runs.
- **In between** (a gain, but under that bar): measured once more. If it is still under the bar, rejected.
- **Long-run attempts.** An attempt whose log entry predicts a gain that grows with run length or needs warm-up (a compile step, a cache) may be judged on one full run of the longest program instead, with start-up included. The entry says so before the measurement.
- **Combined attempts.** Two ideas that only pay off together may be one attempt; the entry says why they are not separate.
- **Groundwork.** A correct attempt that gains nothing but that a later attempt on the plan needs may be kept; the log says which attempt needs it. If that attempt is rejected, the groundwork is reverted too.
- An idea on the plan that is not tried gets a line in the log saying why.

### The attempt families
The profile decides the order and may add or drop items.

| Family | Examples |
|---|---|
| Free wins around the core | Compiler settings (link-time optimisation, native CPU, profile-guided); no per-tick callback or stop check; checking for Ctrl-C rarely. |
| A tighter tick-by-tick core | The wiring in flat arrays; on/off bits packed; components grouped by kind; the data ordered by how signals flow. |
| Optimising the circuit when it is compiled | Dropping what cannot cause an event (LEDs, annotation, logic that only drives them); merging traces and relays; folding constants. |
| Input counting and gates that cannot change | A count of on-inputs per gate; an AND with an input off is not woken by its other inputs. |
| Compiling the circuit | The netlist as a straight-line program or generated machine code per board. |
| Word-wide simulation of repeated slices | Detecting identical bit slices of a datapath and simulating them as one word operation. |
| Lookup tables | A cone of gates as one table lookup, with every path's timing kept exact. |
| Idle-tick jumps | Jumping over ticks in which nothing is pending and no clock or timer flips. |
| Skipping repeated stretches | Remembering what the circuit did from a given full state over a stretch of ticks and replaying it when that state returns. |

In the game every component takes one tick. Anything that reaches a VMem pin, a Breakpoint or a Random component must happen at exactly the tick it happens in v1.

Compiler settings must leave `cargo build --release` (with the documented environment) producing the accepted build; otherwise the new build command goes into `engine/README.md` and `CLAUDE.md` in the same commit. Profile data is not committed.

## Data
- v2's code lives in new files under `engine/src/`. In v1's files only the trait gains the event primitive; how v1 computes a tick does not change.
- The log, `specs/vcb-engine-v2-log.md`:
  - a summary table at the top: attempt number, name, score, verdict;
  - the profile, the noise, the benchmark set with its `AoC2025` commit, and the ranked plan;
  - one entry per attempt, oldest first: the idea in two sentences, the expected gain and why, what was changed, the table (every program: old speed, new speed, change; the score; start-up), the equality command with its pass count, the verdict with its reason, the commit or the tag;
  - a list of ideas not tried, each with its reason;
  - one table per finished stage.
- The benchmark programs are in the separate `AoC2025` repository. The benchmark script fails, naming what is missing, when the folder or a program is absent; it never skips. `cargo test` does not depend on that folder: the day 1 programs are in the engine's own fixtures.

## Interface
```
vcb run <board.vcb> [--engine v2|v1] [--verify] ...
python3 engine/tools/bench.py --old <binary> --new <binary> [--old-engine v1|v2] [--new-engine v1|v2] [--full]
```
- `bench.py` runs the benchmark set alternately on two builds (`--old` and `--new`, each with the engine named for it; the same binary twice compares v1 with v2) and prints per program: ticks, both speeds, the ratio, start-up, and whether events, stop and VMem were equal. Its last line is the score. Any unequal row makes it exit with an error. `--full` runs to the Breakpoints.
- `--engine` with another value: `--engine v3: expected v1 or v2`.
- `--replay-cache <file>` (v2.1 step 5): the replay table is read from the file before the run and written to it after. The file carries a fingerprint of the circuit as the engine holds it, of its settings, and of the engine's own executable: a table is only as right as the engine that made it, so any other build ignores and replaces it. Every number in the file is checked when it is read (entity numbers, node numbers, addresses, lengths), because the tick's loops index without bounds checks; a damaged file is an error. The file is machine-specific (native byte order) and can be as large as `--replay-memory`.

## Errors
- A difference between v1 and v2 in any test or `--verify` run is a failure that names the board (or the random seed), the tick of the first differing event, and what differed.
- To find where two engines part ways: the event sequence is compared in order, so the first differing event gives the tick; `--ticks` on both engines narrows it further.

## Acceptance criteria
Scaffolding:
- [ ] The trait change is its own commit; every existing test passes without being edited, apart from the `--engine` cases in `tests/cli.rs` that asserted "v2 engine does not exist yet".
- [ ] `--engine v2` and `--engine v1` both work. With `--probe` or `--render` and no explicit engine, the run uses v1, prints the notice on standard error, and standard output is byte-identical to `--engine v1`.
- [ ] Standard output and the `--dump-vmem` file of `--engine v2` are byte-identical to `--engine v1` for `--watch`, `--verbose` and the stop line on the command-line test boards.
- [ ] Attempt 0 (v2 as a copy) scores within the measured noise of v1.

Equality, required before any attempt is measured and at every commit. One command runs all of it: `cargo test --release -- --include-ignored`.
- [ ] **Fixtures:** every fixture in `engine/tests/boards/` that v1 runs gives the same events, stop tick, reasons and final VMem on v2, to its natural stop and to three seeded `--ticks` values.
- [ ] **The CPU:** on the two day 1 fixtures (`cpu-aoc1p1.vcb`; part 2 assembled onto it), v2 equals v1 in the whole event sequence to the Breakpoint; at 20 seeded `--ticks` values, at least half not multiples of 256; with `--break-addr` (fetch, write and any) on a program word, a data word written many times and a word never touched; with `--quiescent` below and above the longest gap between writes; and when `--ticks` equals the Breakpoint tick (both reasons, same order).
- [ ] **Resuming:** running v2 to tick N and on to tick M gives the same events and VMem as running straight to M, for 100 seeded cases.
- [ ] **Random boards:** a seeded generator makes at least 1,000 circuits at the component level (`Components`, no pixel layout). Each records sampled signals into VMem or drives a Breakpoint, so every circuit has events. Each is run on v1 and v2 to a seeded tick count up to 10,000 or its first Breakpoint; events, stop and VMem must be equal. A failure prints the seed. The test asserts and prints minimums: at least 500 circuits still produce events after tick 100; at least 150 write VMem and load back a written word; at least 100 reach a Breakpoint; at least 100 have Random ink that draws ten times or more, each run with three seeds; at least 50 have Timer ink.
- [ ] **Long-running circuits:** at least one generated class loops long enough for every cache and table to be used, and is run once with the memory limit set so low that the caches fill (a "full" counter above zero).
- [ ] **Every optimisation fires:** each accepted optimisation has a counter of how often it applied. For each, at least one equality test asserts the counter is above zero. An optimisation that no test triggers is not accepted.
- [ ] **The comparison works:** a test-only fault in v2 (one write event's value changed, one event's tick shifted by one, the stop tick shifted by one) makes the equality check fail each time, and the message names the board, the tick and what differed.
- [ ] **Refusals:** every existing test that expects an error (unverified ink, Timer without `--timer-ticks`, VMem options on a board without VMem, a bad `--break-addr`) runs for both engines with the same message.
- [ ] **Timer:** a Timer board with `--timer-ticks` 1, 7 and 1000, run past three flips each, including a stop on a flip tick and on the tick before.
- [ ] **Ctrl-C:** a long run on v2 that receives an interrupt exits within two seconds with the reason `interrupted` and its outputs; the reported tick T and dumped VMem equal those of `--engine v1 --ticks T`.
- [ ] Plain `cargo test` (debug build) runs the fixtures and at least 100 random circuits on both engines in under a minute and needs no `AoC2025` folder.

Per stage:
- [ ] Every benchmark program run to its Breakpoint with `--verify`: no difference. The table is in the log and in `engine/README.md`.
- [ ] The equality tests pass once with debug checks switched on.
- [ ] Peak memory of the longest run is in the stage table and under 2 GiB.
- [ ] Every attempt of the stage has a complete log entry; every accepted one names its commit and every rejected one its tag; `git status` is clean and the engine's files differ from the last accepted commit by nothing.
- [ ] Stage 1 only: its last commit makes v2 the default.

The log:
- [ ] The summary table agrees with the entries.
- [ ] Every family in the table above is either covered by an entry or listed under ideas not tried, with a reason.

## Rollout
| Milestone | Contents | Ends with |
|---|---|---|
| Profile | The measurements, the noise and the ranked plan, in the log | a stop: the user approves or reorders the plan. Written 2026-11-18; waiting for the user |
| Scaffolding | The trait change; v2 as a copy; `--engine v2`; `--verify`; the equality tests and the generator; the benchmark script | commits; attempt 0 in the log |
| Stage 1, 2, … | The attempts of each stage, as the approved plan groups them | full runs, the table, a stop for the user's decision |

- This spec and the empty log are committed before the profile starts. Each accepted attempt is its own local commit with all tests passing. Nothing is pushed.
- `--engine v1` is the way back at any time.
- Changes to other documents, made with this spec:
  - `vcb-engine.md`: the Status line and the "v2 engine" line point here; "multithreading" and "multithreaded" are dropped; "compared with v1 and the game" becomes "compared with v1"; the benchmark line (median of five runs of a million ticks) and the differential-test criterion (states equal after every tick) point to this spec's rule and tests; `--engine v1|v2  # v2 is an error until it exists` loses its comment once Scaffolding is done.
  - `vcb-engine-v1.md`: the remark that the differential test "compares two independent mechanisms" notes that v2 starts as a copy and diverges by attempts.
  - `engine/README.md` (after Scaffolding and each stage): the `--engine` and `--verify` flags, the latest stage table, and the rule that a suspect v2 result is checked with `--engine v1` or `--verify`.
  - `CLAUDE.md`: the state, the log's place, the rule that a v1 model change must reach v2 in the same commit, and that a new puzzle's first answer is confirmed with `--verify`.
  - `AoC2025/README.md` (the user's repository; changed at stage ends): its Performance times are labelled as v1 times, or replaced by the stage table.

## Open questions
- **Random ink and reordering** (owner: the engine author; before the first attempt that changes evaluation order): can an attempt keep v1's draw order cheaply, or does it switch off on boards with Random ink? The safe default is to switch off. No benchmark program uses Random.
- **The stage boundaries and each attempt's effort estimate** (owner: the engine author proposes, the user approves; at the end of the profile).

## Decision log
- Target → as fast as possible, no number.
- Exactness → the same results at the points a user can observe; shortcuts in between are allowed.
- Coverage → any board; rare cases may fall back to v1.
- Code style → speed first.
- Approach → profile first, then decide.
- Stop rule → the user decides after each stage.
- Proof → every fixture, every day program, and at least 1,000 random circuits.
- Default engine → v2, with v1 on request.
- Native in v2 → the stops and VMem outputs, and Random and Timer inks.
- Resources → one run on one core, memory up to a few GB.
- Benchmark → all day programs, the two builds back to back.
- End result → a file with every optimisation attempt, its effect and whether it was accepted.
- Log entry → the full form, in `specs/vcb-engine-v2-log.md`.
- Acceptance → a rule applied by the engine author: faster by a margin and all tests pass.
- Autonomy → a whole stage, then stop.
- Attempt families → the nine in the table; the user added optimising the circuit network at compile time as a good candidate.
- Measuring → short runs per attempt, full runs per stage.
- Starting point → a copy of v1's core in new files.
- Profile → numbers plus a ranked plan, then the user's go.
- Settled parts → the dirty list already skips them; active parts that cannot cause an event are the candidates.
- Review, equality → the same events, not the same internals. Full-state equality is dropped; `--verbose` becomes native.
- Review, Random ink → stays native (the review proposed running such boards on v1).
- Review, default → v2 becomes the default after Stage 1's full runs, and a `--verify` flag compares a run with v1.
- Review, rejected code → committed on a local tag, then removed from the tree (changed from "removed").
- Review, measuring → the last accepted build against the new one, interleaved, five runs each, to the Breakpoint or 100 million ticks; at least 3% and more than twice the measured noise; long-run attempts may be judged on a full run.
- Review, safety → everything is allowed without asking (the review proposed approval for generated machine code and new dependencies).
- Review, clear fixes → applied: the event contract and stopping inside a skipped stretch; what a remembered state contains; tests that each optimisation fires and that the comparison catches a planted fault; the notice on standard error and the engine named in `--stats`; the 2 GiB limit with a test; how rejections and stage failures are undone; combined attempts and the re-check at stage ends; the generator at the component level; the other documents' lines; milestone names.
