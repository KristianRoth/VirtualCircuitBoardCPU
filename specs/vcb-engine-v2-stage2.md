# VCB engine v2: stage 2

Stage 1 of the fast engine ended on 2026-11-19. This file is the starting point for stage 2: what the engine does now, everything that was tried and what it gave, what was learned about where the time goes, and every idea that is still open or was closed. New ideas from the brainstorming go in the last section.

The rules of the work are in `vcb-engine-v2.md` (what "equal to v1" means, the acceptance rule, the rule that the engine optimises the simulator and not a board). Every attempt's full entry, with its table, is in `vcb-engine-v2-log.md`. This file summarises; where the two disagree, the log is right.

## Where stage 1 ended

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

- **v2 is the default engine** from the end of stage 1. `--engine v1` runs the reference. `--verify` runs both and fails at any difference in the VMem events, the stop or the final VMem.
- With `--probe` or `--render` a run is done by v1 (it says so): v2 does not keep the signals inside stretches it skips.
- Equality is tested by `engine/tests/v2.rs`: v2 against v1 on every fixture board, tick by tick and moved by events with limits of many sizes, and on a whole program (day 1 part 1, 43 million ticks), once with all the memory for replay and once with 3 MiB.

Not done from the spec's end-of-stage list: re-measuring each accepted attempt by switching it off. The attempts build on each other (input counts carry the wake rule, the schedule carries the idle jump), so most cannot be switched off alone any more.

## What v2 does now

**When a board is loaded**
1. The netlist is built as for v1: traces, components, pins, who reads and writes what.
2. Everything that cannot reach a VMem pin, a Breakpoint or a Random is dropped (attempt 6). On the user's CPU: 69% of the entities.
3. The self-running part is found (everything a clock drives that no pin, Random or Timer can influence), run once on the reference engine until it repeats, and replaced by a schedule of the flips it hands to the rest (attempt 11). On the user's CPU: 23 entities, period 64 ticks; on the sample computer: 156 entities, period 128.
4. A trace with exactly one writer is merged into that writer (attempt 10).
5. The wiring goes into flat arrays (attempt 3); each gate gets the count of its inputs that are on and the count at which its output changes (attempts 4 and 5).

The user's CPU has 10,593 entities; about 2,100 are simulated.

**During a run, in this order**
1. **Replay** (attempt 8). At every VMem load, write or Breakpoint the full state is looked up. From a state seen before, with the same word coming from VMem, the engine goes to the next event without the circuit.
2. **Idle-tick jump** (attempt 7). With nothing queued, straight to the next tick at which the schedule, a clock or a timer flips something.
3. **One tick of simulation.** The queued components are evaluated by comparing a count with a threshold; the ones that change flip; the counts of what they feed are adjusted; a gate whose output cannot change from that is not queued. No allocation (attempt 1), no divisions or pin scans (attempt 13), no bounds checks in a release build (attempt 14).

**Off in special cases:** replay on a board with Random ink or a simulated Timer; the trace merge and the schedule on a board with Random ink (every flip must come in v1's order there).

## Every attempt

Gains are against the engine just before the attempt, on the ten-program benchmark (score = geometric mean). "Quick" is one run per program over the first 20 million ticks.

| # | Attempt | Gain | Measured how | Verdict |
|---|---|---|---|---|
| 0 | Scaffolding: v2 as a copy of v1's core | – | – | kept |
| 1 | No allocation in a tick | 1.68 | quick | accepted |
| 2 | Compiler settings (link-time optimisation, native CPU) | 1.004 | quick | rejected |
| 3 | The wiring in flat arrays | 1.085 | quick | accepted |
| 4 | Input counting | 1.072 | three runs | accepted |
| 5 | Gates that cannot change are not woken | 1.16 | quick | accepted |
| 6 | Dropping what cannot cause an event | 1.35 | quick | accepted |
| 7 | The event primitive and idle-tick jumps | 1.046 | three runs | accepted |
| 8 | Replay of repeated steps | 2.63 | full runs | accepted |
| 9 | Chaining replays | 0.98 | quick | rejected |
| 10 | Traces with one writer merged into it | 1.05 | three runs | accepted |
| 11 | The self-running part played from a schedule | 1.65 | quick | accepted |
| 12 | Part-level skipping: units with their own tables | 0.76 to 0.98 against plain simulation | 20 million ticks, two programs | rejected |
| 13 | No fixed work in a tick | 1.055 | three runs | accepted |
| 14 | Indexing without bounds checks in the tick | 1.14 (1.30 on a full run of day 4 part 2) | quick | accepted |
| 15 | Compiler settings, tried again | 1.038 | three runs | accepted |

Rejected code is on local tags: `v2-attempt-02-rejected`, `v2-attempt-09-rejected`, `v2-attempt-12-rejected`.

## What was learned about where the time goes

These numbers decided what was worth trying. They are from the user's CPU unless a board is named.

- **The circuit is sparse in time.** v1 evaluated about 10 components per tick out of 5,533, and half of those did not flip. After the netlist work about 3 flips per tick are left in what v2 simulates.
- **Most of the board cannot matter.** 69% of the entities cannot reach VMem or the Breakpoint; a large share of v1's per-tick work was there.
- **The clock divider was 60% of the remaining flips**, in 23 entities that repeat every 64 ticks.
- **The state repeats.** With the state taken at every VMem event, the share of ticks inside steps seen before is 46% (day 1 part 1), 83% (day 2 part 2), 60% (day 4 part 2), 92% (sample computer). It depends on how many different register contents a program passes through.
- **Size is not cost.** Removing 36% of the simulated entities (attempt 10) gave 5%. An entity that does not change costs nothing; a change costs.
- **Fine parts repeat but have nothing to skip.** Parts of a handful of gates recognise their situation over 95% of the time, but an input change is followed by one or two flips, sometimes none: cheaper to simulate than to look up.
- **A tick costs more than its flips.** Before attempts 13 and 14 a simulated tick cost about 47 ns with about 3 flips in it. Removing the fixed work gave 5%, removing the bounds checks 14% to 30%: the cost was spread through the loop's code, not in one place.
- **Estimates that count events miss the machinery.** Twice an estimate was too high because it counted what would be skipped and not what the skipping costs (attempt 4: the count updates; attempt 12: thawing a unit and learning its table). Estimates that measured a whole mechanism on a real run (replay's hit rate, the self-running part's share) held.

## Ideas that are still open

None of these has been measured unless it says so.

**A cheaper tick**
1. **One record per entity.** A gate's operation, count, threshold, state and queued flag are in five arrays; one 8-byte record touches one cache line.
2. **Renumbering.** The 2,100 simulated entities are scattered among 10,593 numbers. Numbered together, with 16-bit indices in the lists, they fit the fastest cache; packing a state for replay becomes a copy.
3. **Profile-guided optimisation.** The compiler settings of attempt 15 without it gave 4%. A build that has seen a real run can lay the hot loop out better. Needs a recorded profile, which the spec says is not committed, so the build would need a step.
4. **The latch's scan.** A latch or pin still looks through its inputs for the ones that just rose, where gates compare a count. Latches were 20% of v1's evaluations.
5. **Stop and restart costs in `advance`.** Every pass of the loop checks for a replayable node, syncs, tests the idle condition and tests for an event. Not profiled since attempt 8.

**More replay**
6. **A replay table that survives between runs.** The stored states depend only on the board, so a second run, or another program on the same board, starts warm. A file of up to 2 GB.
7. **The loaded word masked to the memory's content width.** A few more hits; probably tiny on the user's CPU.
8. **Replay on a board with a simulated Timer.** The compact sample gets none today, because the timer's position would be part of every state. It would have to be left out where it cannot matter.
9. **Fewer lookups during a run of misses.** Packing and looking up the state was about 15% of the time in simulated stretches (measured after attempt 8, before 10 to 15).
10. **Replay that ignores what a step never reads.** A step is reused today only when the whole state matches. If the step recorded which signals it read, it could be reused in any state that agrees on those. Attempt 12 was the version of this with fixed parts and lost on its machinery; a version keyed on what a step actually read has not been tried or measured.
11. **The memory limit.** Replay stops learning at 2 GiB. Day 4 part 2 stays under it (670 MB at attempt 8). A longer program may not; dropping states that are never reused has not been needed yet.

**Other**
12. **More than one core.** No way seen: each step depends on the one before. Running two programs at once already works, as two processes.

## Ideas that are closed, and what closed them

- **Compiling the whole circuit to straight-line code, word-wide simulation of bit slices, lookup tables for cones of gates.** 0.2% to 0.4% of the components work in a tick, so an engine that touches all of them starts hundreds of times behind.
- **Chaining replays** (attempt 9). The return to the run loop is not where the time is: 0.98.
- **Part-level skipping** in every form measured:
  - a fixed level of parts: only the coarsest (two to seven parts) pays, 5% to 25% of what replay leaves on the user's CPU;
  - many fine parts with hashed lookups: 1 to 2 flips per lookup;
  - fine parts read from a table: 0.2 to 0.9 flips per lookup, because inputs change more often than the part flips;
  - a tree whose depth follows the parts, chosen with the run known: 15% to 51% of what replay leaves, 0.45 to 0.82 flips per tick;
  - a tree fixed at start-up by size alone: no setting good on all boards;
  - built (attempt 12): slower than plain simulation at every unit size, because thawing a unit after a failed lookup and learning its table cost more than the flips saved.
- **Removing duplicate gates and constants from the netlist.** 12 duplicates and 37 constants among 3,279 simulated entities on the user's CPU.

## Tools made in stage 1

All in `engine/examples/`, run from `engine/` with `cargo run --release --example <name> -- ...`. They use only the engine's public interface.

| Tool | What it answers |
|---|---|
| `profile` | How many components work per tick, how often states repeat, speed over a whole run |
| `replay_estimate`, `replay_stats` | How often replay would hit; what it did on a run (speed, share replayed, memory) |
| `live` | How much of a board v2 simulates |
| `netlist_stats` | What a netlist optimisation could remove: single-writer traces, duplicates, constants |
| `heatmap` | A picture of the board coloured by how often each part flips |
| `autonomous`, `clock_fan` | The self-running part of a board, its period, where it hands over |
| `parts`, `parts_tree`, `parts_estimate` | Sparsely connected parts as a tree, drawn on the board; what part-level skipping could skip at any setting |
| `tools/bench.py` | The benchmark: two builds or two engines alternately on the ten programs |

## How the work was done

- **Measure before building** anything expensive: replay, the schedule and part-level skipping each got an estimator first. Two of the three estimates held.
- **The quick benchmark first** (one run, 20 million ticks, about a minute). A clear result is decided from it; a result near the 3% bar is repeated with three runs; an attempt that needs warm-up is judged on full runs.
- **Noise:** the same build against itself scores within 0.5% of 1 over the ten programs, although single programs move by 2% to 3%. Nothing else heavy may run during a measurement.
- **The equality tests are the proof.** Three faults were caught by them or by the engine's own checks before anything was measured: a timer interval leaking into every replay state (in a measuring tool), a schedule table without an entry for tick 0, and a weak hash for a set of flipped inputs.
- **The engine optimises the simulator, not a board.** Nothing in it comes from knowing the user's CPU; the benchmark keeps boards of three designs.

## New ideas for stage 2

(To be filled in from the brainstorming.)
