# VCB engine v2.1

The first stage of the fast engine (v2) ended on 2026-11-19. What comes next is called **v2.1** (the user's name for it; "stage 2" in older text means the same). This file is its starting point: what the engine does now, everything that was tried and what it gave, what was learned about where the time goes, and every idea that is still open or was closed. New ideas from the brainstorming go in the last section.

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

## New ideas for v2.1

Collected broadly on 2026-11-19, to be tried or pruned later. Nothing here is measured on this engine. Each idea says where it comes from, because that decides how far it can be trusted:
- **researched**: from the web research of 2026-11-19, where each claim about a source was checked three times against the source. What the source says is reliable; whether it fits this engine is an inference.
- **not researched**: an angle the research was asked about but did not bring to a checked claim, or an idea of our own. Not ruled out, not supported.

The research was narrower than asked: 24 checked claims from six sources. It covered the fast open simulators, memoization in CPU simulators and two abstraction methods. It brought nothing checked on Hashlife, incremental hashing, data layout, compiled hot traces, event-queue structures, GSIM, Verilator's internals or GPU and FPGA.

### A. More of the run replayed

**A1. Replay keyed on the control part of the state, with the data checked on the way (researched; the one new direction with real promise).**
FastSim (ASPLOS 1998) and Facile (PLDI 2001) are CPU simulators that memoize like v2 does, with one difference: the key is only the control part of the state, without data values. A cache entry holds the actions to perform, not a result state. Where a data value decides the path, the entry branches: the value is compared with the ones recorded, and an unseen value is a miss that goes back to the slow simulator, which adds the branch. FastSim replayed 99.7% to 99.999% of instructions and gained 4.9 to 11.9 times over the same simulator without memoization; v2 replays 45% to 92% of ticks. The difference is plausibly the control-only key: v2 misses whenever any register bit differs.
- Sources: https://ftp1.cs.wisc.edu/wwt/asplos98_fastsim.pdf, https://research.cs.wisc.edu/multifacet/papers/pldi01_facile.pdf, https://pages.cs.wisc.edu/~wwt/fastsim/FastSim_v2.html
- What does not transfer by itself: FastSim gets its data values from a separate executor of the program, and Facile gets the split from analysing the simulator's source. A netlist has no ready-made split into control and data, and v2 would still have to carry the data signals through a replayed step (a register copy has to copy). So the gain is the control logic's work, not all of it.
- What would have to be found out first, with an estimator as for replay: which state bits can be left out of the key without two different futures sharing a key (candidates: bits that differ between states which otherwise agree; bits the step never read), and how much of what replay misses today such a key would recover.
- Related: open idea 10 (replay that ignores what a step never reads) is the same aim from the other side. Attempt 12 failed on machinery per part per tick; here the bookkeeping would be per step, about 160 ticks, which is a different balance.

**A2. Bounding the replay table by flushing (researched).** FastSim's cache reached 889 MB on one program and the paper bounds it by throwing entries away. v2 stops learning at 2 GiB and keeps everything. Flushing, or dropping states never reused, is what a longer program would need. Source: the FastSim paper above.

**A3. Carried over from the list above:** the table saved between runs (6), the loaded word masked (7), replay with a simulated Timer (8), fewer lookups during a run of misses (9).

**A4. Warming the table on other cores (not researched; our own).** Replay misses when a state meets a word it has not seen. Idle cores could simulate ahead from recent states with likely words (the next words of the program, neighbouring data) and put the results in the table before the main run gets there. Speculative: it needs a table several threads can write, and a way to guess words that is right often enough.

**A5. Hashlife-style memoization in space and time (not researched).** Hashlife speeds up cellular automata by memoizing blocks of cells over spans of time that double. Whether anything of it carries over to an irregular netlist was asked and not answered. Attempt 12 (parts with their own tables) is its nearest relative and lost.

### B. A cheaper simulated tick

**B1. Passes from Arcilator that do not depend on its whole-circuit design (researched; fit unmeasured).** Arcilator is a compiled, every-signal-every-cycle simulator, the kind already ruled out here, but some of its passes stand alone: one piece of code for structurally identical blocks of logic; a deliberate layout of the state in memory; grouping stores that share an enable; turning multiplexers into branches. The slides give no gain per pass. Its retiming pass is not usable: it moves a signal's change to another tick. Source: https://llvm.org/devmtg/2023-10/slides/techtalks/Erhart-Arcilator-FastAndCycleAccurateHardwareSimulationInCIRCT.pdf

**B2. Branch hints and a small code footprint (researched).** In ESSENT a large share of the gain on one design (2.9 times) came from hints that kept the hot code's working set inside the cache. Cheap to try here: mark the rare paths of the tick (VMem, Breakpoint, Random) as cold. Source: https://escholarship.org/uc/item/64x147r1

**B3. Fusing a write with the read that follows (researched; low).** Khronos joins the write of a state element in one cycle with its read in the next to save memory accesses. Here the nearest thing is not storing a signal that is consumed in the next tick. With 3 changes per tick and a state that fits the first cache, the gain is probably small. Source: https://github.com/pku-liang/ksim (the paper itself was not read).

**B4. Carried over:** one record per entity (1), renumbering with 16-bit indices (2), profile-guided optimisation (3), the latch's scan (4), the event loop's own overhead (5).

**B5. Native code for event sequences that keep recurring (not researched; our own).** The same short sequences of flips happen again and again inside simulated stretches. A recurring sequence could be compiled once to straight-line code that applies its flips and count changes without the queue. It is replay at a finer grain without a lookup per tick; the cost is a compiler in the engine and knowing when a sequence starts.

**B6. A state hash kept up to date flip by flip (not researched).** Today a state is packed from 2,100 signals at every event of a simulated stretch before it is looked up. A hash updated at each flip (one XOR, as in chess programs' Zobrist hashing) would make the lookup free. The catch is the rule that a state is reused only after its whole recorded content has been compared: a hit still has to pack and compare, so only misses get cheaper, unless states are stored as differences from an earlier state.

### C. A different model of the circuit

**C1. Unrolling a fixed number of ticks symbolically and simplifying (researched; as published it does not fit).** "Clock abstraction" (Jain, Bryant and Jain, DAC 1995) turns a unit-delay gate circuit into a per-cycle model by simulating one clock cycle with symbols and merging nets that are equal; 40% to 70% fewer gates and 3 to 150 times in the simulator of the day, on circuits of 184 to 9,927 gates. As published the user supplies the clock and input timing and names the outputs and sample times, and outputs are right only at those times, so it is neither automatic nor tick-exact. The idea underneath is worth keeping: v2 already knows the schedule's period, and what the circuit does over one period, as a function of the state at its start, could be worked out once and simplified, with the tick of each VMem event kept. Whether that stays small on a 2,000-element CPU is unknown. Source: https://cecs.uci.edu/~papers/compendium94-03/papers/1995/dac95/pdffiles/40_4.pdf

**C2. An instruction-level model derived from the gates (researched; does not fit as published).** Instruction-level abstraction synthesis (FMCAD 2015) needs a hand-written template per design, and it agrees with the circuit only at the end of an instruction, not in between. No checked source derives an instruction simulator from a gate-level CPU automatically with every tick's events kept. Source: https://cse.iitk.ac.in/users/spramod/papers/fmcad15.pdf

### D. Looked at and not for this engine (researched)

- **Activity tracked per partition** (ESSENT, 1.5 to 11.5 times over Verilator): its own finding is that tracking per signal costs more than it saves, so it tracks per partition of tens to hundreds of elements, in a design that otherwise evaluates everything. Its activity is 1.1% to 2.1% of signals per cycle against 0.2% to 0.4% here, and partitions were measured and rejected here (attempt 12 and the estimates before it).
- **Splitting the circuit over threads** (RepCut, 27 times on 24 threads): measured on a design of 1.2 million nodes, self-relative, and the gain comes from each core's code fitting its cache. On small designs it gains much less, because a short cycle cannot pay for the synchronisation; the smallest design tested has 70,000 nodes. A tick of a few nanoseconds leaves nothing to pay with.
- **Whole-circuit compiled simulation** (Verilator, Arcilator): already ruled out by the share of the circuit that works in a tick.

### The order to try them

Proposed 2026-11-19; the user decides. Ordered by what an idea could give, how sure that is and what it costs, with the rule from stage 1 kept: anything expensive gets a measurement before it gets built. Ideas are named by their letter above or their number in "Ideas that are still open".

| Step | What | Why here | Effort | What decides the next step |
|---|---|---|---|---|
| 0 | **A fresh profile of v2 as it is now**: per program, the share of time in replayed steps, in simulated ticks, and in packing and looking up states; the cost of a tick and of a flip | The last profile is from before attempts 10 to 15. Every estimate below rests on these numbers | hours | Which of steps 2 and 3 can matter at all |
| 1 | **Estimator for control-keyed replay** (A1, open idea 10): which state bits can leave the key without two futures sharing it, and how much of today's misses that recovers, on all three boards | The only idea with a published result far above where v2 is (99.7% replayed against 45% to 92%). Cheap to measure, expensive to build | a day | Build it (step 6) or drop it |
| 2 | **The cheap tick items, one attempt each**: cold paths (B2), the loaded word masked (7), the latch's scan (4), the event loop's overhead (5), profile-guided optimisation (3) | Hours each, no risk to correctness beyond the equality tests, and attempt 14 showed the tick's code still had slack | hours each | Each is accepted or rejected by the quick benchmark |
| 3 | **Cheaper misses in replay**: fewer lookups during a run of misses (9), a hash kept up to date flip by flip (B6) | Packing and lookup were 15% of simulated stretches when last measured; step 0 says what they are now | a day | Skipped if step 0 shows the share is small |
| 4 | **Layout**: one record per entity (1), then renumbering with 16-bit indices (2), with Arcilator's layout and grouping passes (B1) looked at on the way | Certain but moderate, and every index in v2 changes, so it comes after the small things that would otherwise have to be redone | days | Benchmark |
| 5 | **The table across runs and under a limit**: saved between runs (6), entries thrown away at the memory limit (A2, 11) | Worth as much as the same board is run again; needed in any case before a program outgrows 2 GiB | a day or two | How the user works: repeated runs or new programs |
| 6 | **Control-keyed replay built**, if step 1 says so | The largest possible gain left | a week | Full runs |
| 7 | **Replay on a board with a simulated Timer** (8) | Only the compact sample gains (3.9 times today, the lowest row). Keeps the benchmark honest across designs; nothing for the user's CPU | unknown | An estimator first |
| 8 | **Unrolling one schedule period** (C1), as an estimator | A different model of the circuit; only worth the look if step 1 fails | a day to measure | Whether the unrolled form stays small |
| 9 | **The speculative ones**: warming the table on other cores (A4), native code for recurring event sequences (B5), Hashlife-style memoization (A5), write and read fused (B3) | No source or measurement behind them yet | each needs an estimator first | |

Not to be tried: per-partition activity, the circuit split over threads, whole-circuit compiled simulation, an instruction-level model from a hand-written template (section D and C2).

Two notes on the order:
- Steps 1 and 2 do not depend on each other. Step 2 can fill the time while step 1's estimator runs.
- A second research pass on the angles that brought nothing (Hashlife, incremental hashing, layout, compiled traces, event queues, GSIM) would cost about as much as the first. It is worth it only before step 9; steps 0 to 6 do not need it.

### Step 0 done: the profile of v2 (2026-11-19)

v2 at attempt 15, with `engine/examples/profile2.rs` in a profiling build (counters on, the main functions kept apart for the sampling profiler). v1 is not part of this any more.

**How the ticks are crossed**

| Program | Speed, M ticks/s | Replayed | Jumped as idle | Simulated one by one |
|---|---|---|---|---|
| day 1 part 2 (whole run) | 60 | 45.5% | 39.6% | 14.8% |
| day 2 part 2 (whole run) | 187 | 82.6% | 12.6% | 4.8% |
| day 3 part 2 (whole run) | 129 | 68.1% | 23.2% | 8.7% |
| day 4 part 2 (first 300 million) | 86 | 63.4% | 26.8% | 9.9% |
| sample (100 million) | 455 | 91.8% | 7.3% | 1.0% |
| sample compact (100 million; no replay there) | 51 | 0% | 78.2% | 21.8% |

There is an event (a VMem load or write) every 156 to 163 ticks on the user's CPU. In a step that replay does not know, about 41 of those ticks are simulated and the rest jumped as idle. A simulated tick holds 6 to 7 evaluations and 6 to 7 flips: the ticks that are still simulated are the busy ones.

**Where the time goes** (day 4 part 2, sampling profiler)

| Share of the time | Where | Share of the ticks it covers |
|---|---|---|
| 52% | `propagate`: applying flips to the counts of what they feed, waking gates | simulated: 9.9% |
| 22% | `step_real`: the queue, evaluating, the rest of a tick | the same ticks |
| 13% | packing the state at each event of a simulated step, to look it up | |
| 7% | replay itself: the lookups for a known step, and the run loop | replayed: 63.4% |
| 2% | restoring a state into the circuit when replay ends | |
| 3% | storing new states, copying, memory | |

The same run with replay switched off: 39 M ticks per second, 73% of the ticks idle, 27% simulated, about 80 ns per simulated tick with its share of everything, about 13 ns per flip.

What it says:
- **Three quarters of the time is the simulation of the tenth of the ticks that replay does not cover**, and the largest single piece is `propagate`. Replaying a tick costs about a hundredth of simulating one.
- **Packing the state is the second cost**, 13%, and it is only paid in steps that are simulated anyway.
- The idle jump, which did nothing on this CPU before the schedule (attempt 11), now crosses a quarter to 40% of all ticks for nothing.
- So there are two ways left to gain: fewer simulated steps (more replay) or cheaper ones (`propagate`, the packing).

### Step 1 done: what a key on less than the whole state would recover (2026-11-19)

`engine/examples/readset_estimate.rs`, 20 million ticks per run on the reference engine. A step is reused today only if the whole state at its start was seen before. The estimator collects, for every step, its **read set**: the signals whose value from before the step some gate looked at. A step does the same in any state that agrees on its read set (the circuit is deterministic), so this is a sound rule that needs no knowledge of the board. Two versions: every input of a gate whose input changed counts as read; or, sharper, only the inputs that decide the gate (an AND with an input off does not read its other inputs; a latch reads only itself), which is how a closed enable hides a data bit.

| Program | Whole state seen before (replay today) | Read set seen before | Sharper read set seen before | Signals in a sharper read set, of all that matter |
|---|---|---|---|---|
| day 1 part 2 | 35.7% | 48.4% | 54.6% | 815 of 3,279 |
| day 2 part 2 | 84.6% | 87.0% | 87.5% | 823 of 3,279 |
| day 4 part 2 | 44.3% | 52.3% | 54.8% | 825 of 3,279 |
| sample | 84.1% | 84.1% | 84.7% | 1,461 of 5,124 |
| sample compact | 82.3% | 84.9% | 83.1% | 741 of 3,141 |

(Shares of ticks, over the first 20 million ticks from a cold start, which is why they are below the whole-run figures above. The compact sample's sharper figure is under its plain one because which deciding input is taken is a free choice, and different choices give different keys for the same step.)

**Verdict on A1 as a read-set key: a moderate gain at best, not the jump FastSim reports.**
- On the two weak programs it could raise replay from 36% to 55% and from 44% to 55% of the ticks. In time that is at most about 1.35 and 1.2, since time follows the ticks that are not replayed. On the other three boards and programs it gives 0 to 3 points.
- These are upper limits: they say that a matching step exists, not what it costs to find it. A step's read set is about 820 signals, a quarter of the circuit, so a lookup has to compare hundreds of signals against several candidates.
- Why it is not more: a step of 160 ticks reads a quarter of the circuit, the registers among it. FastSim reaches 99.7% because its data values are computed by a separate executor while only the control is memoized. Leaving out what a step does not read does not do that: a step that moves or adds data reads the data. To get there, a replayed step would have to carry its data through as a function of what it read (copy this bit there, invert that one), which means working the step out with symbols. That is idea C1's territory and has no estimate yet.

Decision for the user: build read-set replay for up to 1.35 on the weakest programs (a week, with the lookup cost unknown), or leave it and take the cheaper steps first. Recommended: leave it for now; step 2 and step 3 attack the 52% and the 13% directly.

### What the research leaves open

1. Can the split into control and data be found from a netlist, and how much of today's misses would a control-only key recover? (Decides A1.)
2. Does unrolling one schedule period stay small on a CPU of this size? (Decides C1.)
3. The angles that brought no checked claim: Hashlife on irregular netlists, incremental state hashing, data layout and renumbering, compiled hot traces, event-queue structures, and GSIM (arXiv 2508.02236), which unlike Verilator and Arcilator skips inactive signals and was not examined.
