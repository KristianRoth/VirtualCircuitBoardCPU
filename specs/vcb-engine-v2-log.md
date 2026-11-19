# VCB engine v2: the optimisation log

Every optimisation that was tried on the v2 engine, what it gained, and whether it was kept. The process, the acceptance rule and what "equal to v1" means are in `vcb-engine-v2.md`.

## Summary

| # | Attempt | Effect on the mean | Verdict |
|---|---|---|---|
| 0 | Scaffolding: v2 as a copy of v1's core | not measured separately (the same code) | kept |
| 1 | No allocation in a tick | 1.68 (quick run) | accepted |
| 2 | Compiler settings | 1.004 (quick run) | rejected |
| 3 | The wiring in flat arrays | 1.085 (quick run) | accepted |
| 4 | Input counting | 1.072 (three runs) | accepted |
| 5 | Gates that cannot change are not woken | 1.16 (quick run) | accepted |
| 6 | Dropping what cannot cause an event | 1.35 (quick run) | accepted |
| 7 | The event primitive and idle-tick jumps | 1.046 (three runs) | accepted |
| 8 | Replay of repeated steps | 2.63 (full runs) | accepted |
| 9 | Chaining replays | 0.98 (quick run) | rejected |
| 10 | Netlist: traces with one writer merged into it | 1.05 (three runs) | accepted |
| 11 | The self-running part played from a schedule | 1.65 (quick run) | accepted |
| 12 | Part-level skipping: units with their own tables | 0.76 to 0.98 against the plain simulation | rejected |
| 13 | No fixed work in a tick: kept counters for clock, timer and schedule; one flag for the VMem pins | 1.055 (three runs) | accepted |
| 14 | Indexing without bounds checks in the tick | 1.14 (quick run) | accepted |
| 15 | Compiler settings, tried again | 1.038 (three runs) | accepted |
| 16 | v2.1: the rare parts of a tick as cold functions | 1.015, then 1.033 on the engine after attempt 19 (three runs each) | accepted after the bar was lowered to 1.015 |
| 17 | v2.1: replay keyed on the loaded word's content bits only | 1.000 (quick run) | rejected |
| 18 | v2.1: a latch keeps a toggle flag; no just-risen marks | 1.045 (three runs) | accepted |
| 19 | v2.1: the packed state kept up to date at every change | 1.11 (three runs) | accepted |
| 20 | v2.1: one record per entity | 1.028 (three runs) | accepted |
| 21 | v2.1: the engine's own numbering, simulated entities first | 0.999 (three runs) | rejected |
| 22 | v2.1: the replay table emptied at its limit; saved between runs (`--replay-cache`) | 1.001 on the benchmark; a second run of day 4 part 2 in 1.3 s | accepted (a feature, not a speed-up of a first run) |

## The profile and the ranked plan

Measured 2026-11-18 on v1 (commit `873d0a1`), with `engine/examples/profile.rs` and `engine/tools/bench.py`. Machine: the user's Mac, with the game running beside it for the noise runs.

### The benchmark set
Ten programs, `AoC2025` at commit `9e8bd62`. Each runs to its Breakpoint or 100 million ticks, whichever comes first.

| Program | Board | Ticks to the Breakpoint | v1, million ticks/s |
|---|---|---|---|
| day1/part1 | the user's CPU (`AoC2025/cpu-32bit-autorun-aoc1p1.vcb`) | 43,146,512 | 6.2 |
| day1/part2 | the same | 70,339,472 | 6.2 |
| day2/part1 | the same | 43,495,312 | 6.2 |
| day2/part2 | the same | 75,753,616 | 6.4 |
| day3/part1 | the same | 170,904,144 | 6.8 |
| day3/part2 | the same | 419,669,712 | 6.4 |
| day4/part1 | the same | 481,327,888 | 6.3 |
| day4/part2 | the same | 1,339,421,264 | 6.2 |
| sample-primes | the game's 32-bit sample computer (`saves/bench/32-bit-cpu-primes.vcb`) | none: runs to the limit | 5.9 |
| sample-compact-primes | its compact version (`saves/bench/32-bit-cpu-compact-primes.vcb`, `--timer-ticks 1000`) | none | 14.3 |

The two sample boards are the game's own and are not in the repository. They are other CPU designs, so a gain that only fits the user's CPU shows as such.

### How much of the circuit works in a tick
| | the user's CPU | sample | sample compact |
|---|---|---|---|
| Entities (traces, components, pins) | 10,593 (5,028 / 5,533 / 32) | 9,113 (4,133 / 4,928 / 52) | 6,465 (2,982 / 3,431 / 52) |
| Components evaluated per tick, mean | 10.3 to 11.0 (all eight programs) | 19.1 | 6.8 |
| The same as a share of all components | 0.19% to 0.20% | 0.39% | 0.20% |
| Median, 99th percentile, most in one tick | 2, about 110, about 290 | 6, 592, 951 | 0, 102, 567 |
| Inputs read per tick in those evaluations | 27 | 62 | 21 |
| Components that flip per tick | 5 | 5.0 | 1.8 |
| Traces that change per tick | 4.7 | 5.0 | 1.9 |
| Entities that ever change | 29% | 76% | 73% |
| Evaluations by ink | AND 61%, Latch Off 20%, OR 6%, LED 4%, XOR 3%, Buffer 2%, NOT 2% | AND 80%, Latch Off 11%, Buffer 6%, NOT 3% | AND 78%, XOR 16%, Latch Off 2% |

What it says:
- **The dirty list already skips settled logic.** v1 touches one component in five hundred per tick. An engine that evaluates the whole circuit every tick, however tightly compiled, starts 500 times behind and would need more than that from bit-packing to win. The compiled-circuit and word-wide families are therefore not on the plan as whole-circuit rewrites.
- **Half of the compact sample's ticks do nothing** (median 0; its clock interval is 36). On the user's CPU the clock interval is 1 and almost no tick is idle.
- Only half of the evaluated components flip (5 of 10.6), and each evaluation reads 2.5 inputs on average: the AND gates are woken by inputs that cannot change their output.

### Where the time goes
The step loop alone (no run loop, no stop checks) runs at 6.39 million ticks per second, and the whole `vcb run` at 6.46: **the per-tick callback and the stop checks cost nothing measurable.** VMem does not show in the profile either (one access per several hundred ticks).

A sampling profile of the step loop on day 4 part 2, 156 ns per tick:

| Share | Where |
|---|---|
| about 35% | memory allocation: `step` builds a new `flips` list and a new `dirty` list every tick, and they grow by reallocation |
| about 23% | `propagate`: recomputing the traces a flipped component writes, queueing their readers |
| about 22% | the rest of `step`: the loop over the dirty list, the kind lookup, the apply pass |
| about 20% | `evaluate`: counting the inputs that are on |

### Repeats
Counted on the first 5 million ticks, with the state after a tick and the state before it as the key. This key leaves out the clock position and the words VMem delivers, so the numbers are an upper bound on what a replay could use.

| | Ticks in a state seen before | Repeated stretches: median length, longest | Ticks inside stretches of 256 or more |
|---|---|---|---|
| day4/part2 | 46.6% | 576, 16,191 | 42.1% |
| day2/part2 | 73.2% | 512, 81,788 | 70.3% |
| sample-primes | 41.1% | 1,791, 15,934 | 41.1% |
| sample-compact-primes | 85.0% | 27, 1,006 | 36.2% |

A stretch of about 512 or 576 ticks is two instructions of the user's CPU. So the CPU often does the same two or more instructions from the same full state, and between 40% and 70% of all ticks lie in such repeats.

### Does the start resemble the rest?
Day 4 part 2, million ticks per second in each window of 100 million ticks, to its Breakpoint at tick 1,339,421,264:

`6.21 6.21 6.28 6.36 6.38 6.40 6.44 6.45 6.39 6.40 6.35 6.33 6.33 6.03`

All within 4% of each other. The first 100 million ticks stand for the whole run as far as v1's speed goes. Whether that holds for a replay cache is not known and is measured when replay is attempted.

### The noise
The same build against itself, as the acceptance rule measures two builds (alternating, five runs each, best of five per program), with the game running on the same machine:

| Run | Score |
|---|---|
| 1 (ten programs) | 1.0010 |
| 2 (ten programs) | 0.9951 |

Single programs moved between 0.978 and 1.032, but the score over ten programs stayed within 0.5% of 1. **The noise is 0.5%.** Twice that is 1%, so the rule's other bar decides: an attempt is accepted at a score of 1.03 or more. Two earlier runs with eight programs, during which builds and tests were running, gave 1.0027 and 0.9901: nothing else heavy should run during a measurement.

### The ranked plan
Gains are estimates for the score, from the numbers above. Effort is working time. Within a stage the order is the order of attempts.

**Stage 1: a tighter tick-by-tick core.** Small, certain, and everything later builds on it.

| # | Attempt | Expected gain | Why | Effort |
|---|---|---|---|---|
| 1 | No allocation in a tick: the `dirty` and `flips` lists are kept and swapped | 1.3 to 1.5 | allocation is 35% of the time | an hour |
| 2 | Compiler settings: link-time optimisation, one codegen unit, native CPU, abort on panic | 1.05 to 1.15 | free; applies to v1 too, which is then re-baselined | an hour |
| 3 | The wiring in flat arrays (one array of readers, writers, reads, writes with offsets) and the kind as one byte | 1.1 to 1.3 | every list is its own heap block today; propagate and evaluate chase pointers | half a day |
| 4 | Input counting: each component keeps the number of its inputs that are on, each trace the number of its writers that are on | 1.2 to 1.4 | removes the 27 input reads per tick and the writer scans; evaluate becomes a comparison | a day |
| 5 | Gates that cannot change are not woken: an AND with another input off, an OR with another input on | 1.05 to 1.2 | half of the evaluations flip nothing; needs attempt 4 | half a day |
| 6 | Dropping what cannot cause an event when the circuit is compiled: LEDs and logic that only drives them, merged buffers | 1.03 to 1.05 | LEDs are 4% of the evaluations on the user's CPU | a day |
| 7 | Idle-tick jumps: when nothing is queued, go straight to the next clock or timer flip | 1.0 on the user's CPU, up to 2 on the compact sample; about 1.05 on the score | half of the compact sample's ticks are idle | half a day |

Together roughly 2 to 3 times v1. Stage 1 ends with v2 as the default, as the spec says.

**Stage 2: replaying repeated stretches.** The one family that can go beyond a constant factor.

| # | Attempt | Expected gain | Why | Effort |
|---|---|---|---|---|
| 8 | Record what the circuit does from a full state until the next VMem load, and replay it when the same full state returns | 1.5 to 3 on top of stage 1 | 40% to 70% of ticks lie in repeats of 256 ticks or more; a replayed stretch costs a table lookup and its events | several days |
| 9 | Chaining replays: a replayed stretch that ends in a known state continues with the next without leaving the table | unknown; measured after 8 | removes the per-stretch overhead when whole loops repeat | a day |

Attempt 8 is judged on a full run of day 4 part 2 with start-up included, as the rule allows for attempts that need warm-up. Its real gain depends on how many different full states the CPU passes through, which the profile's key overstates.

**Not planned, with the number that rules it out:**
- The whole circuit as a straight-line program or generated machine code, and word-wide simulation of bit slices: 0.2% to 0.4% of the components work in a tick.
- Lookup tables for cones of gates: the same number; a table saves evaluations, and there are only ten per tick.
- No per-tick callback, rarer Ctrl-C checks: the run loop costs nothing measurable. The event primitive is still built in Scaffolding, because replay needs it.

## Attempts

### 0. Scaffolding (2026-11-19, commit `8bf45ee`)
v2 is a copy of v1's core in `engine/src/sim2.rs`; `vcb run --engine v2` runs it and `--stats` names the engine. With `--probe` or `--render` the run is done by v1, with the notice line. Equality: `tests/v2.rs` steps both engines side by side and compares every signal after every tick, on every fixture board and (ignored by default) over the 44 million ticks of day 1 part 1.

Not done yet from the spec's Scaffolding: `--verify` (the event primitive came with attempt 7). The profile showed the per-tick run loop costs nothing measurable, so attempts 1 to 6 do not need the primitive; it is due before idle-tick jumps or replay. v2 against v1 was not measured as its own row: it is the same code, and the "old" column below (5.9 to 6.4) is v1's speed.

### 1. No allocation in a tick (2026-11-19): accepted
**Idea.** `step` built a new `flips` list and a new `dirty` list every tick. v2 keeps both between ticks (`work`, `flips`) and swaps them, so a tick allocates nothing.

**Expected.** 1.3 to 1.5: allocation was about 35% of the time in the profile.

**Changed.** Twelve lines in `sim2.rs`: two fields, a swap at the start of `step`, the lists handed back at its end.

**Measured.** At the user's request a quick run, not the rule's five: one run per program, 20 million ticks, old = attempt 0 (`--engine v2`), new = this attempt. The noise of such a short run was not measured; the result is far outside anything seen in the noise runs.

| Program | Old, M ticks/s | New | Ratio |
|---|---|---|---|
| day1/part1 | 5.88 | 9.73 | 1.653 |
| day1/part2 | 5.91 | 9.86 | 1.670 |
| day2/part1 | 5.86 | 10.14 | 1.729 |
| day2/part2 | 5.90 | 10.49 | 1.778 |
| day3/part1 | 6.36 | 11.73 | 1.846 |
| day3/part2 | 6.42 | 11.14 | 1.734 |
| day4/part1 | 6.24 | 11.08 | 1.777 |
| day4/part2 | 6.38 | 10.43 | 1.634 |
| sample-primes | 5.92 | 9.45 | 1.597 |
| sample-compact-primes | 14.47 | 20.51 | 1.417 |
| **Score** | | | **1.68** |

Start-up unchanged (0.06 to 0.08 s). Every program stopped at the same tick with the same output on both builds.

**Equality.** `cargo test` (71 passed) and `cargo test --release --test v2 -- --include-ignored` (2 passed).

**Verdict.** Accepted: 1.68, above the estimate. Freeing and growing the lists cost more than the profile's 35% suggested, probably because the allocator calls also disturbed the cache between ticks.

### 2. Compiler settings (2026-11-19): rejected
**Idea.** Let the compiler optimise the whole program at once and for this machine's CPU: `lto = "fat"`, `codegen-units = 1`, `panic = "abort"` in the release profile, and `-C target-cpu=native`.

**Expected.** 1.05 to 1.15, for both engines.

**Changed.** Six lines in `engine/Cargo.toml` and a new `engine/.cargo/config.toml`. Build time went from 4 to 19 seconds.

**Measured.** Quick run (one run per program, 20 million ticks; from now on every attempt gets this first, at the user's request). Old = attempt 1, new = this attempt, both `--engine v2`.

| Program | Old, M ticks/s | New | Ratio |
|---|---|---|---|
| day1/part1 | 10.56 | 10.53 | 0.997 |
| day1/part2 | 10.54 | 10.61 | 1.006 |
| day2/part1 | 10.88 | 10.71 | 0.985 |
| day2/part2 | 10.93 | 10.59 | 0.969 |
| day3/part1 | 11.14 | 10.68 | 0.959 |
| day3/part2 | 10.10 | 11.60 | 1.149 |
| day4/part1 | 11.38 | 11.27 | 0.990 |
| day4/part2 | 10.87 | 10.84 | 0.997 |
| sample-primes | 9.59 | 9.38 | 0.978 |
| sample-compact-primes | 22.27 | 22.78 | 1.023 |
| **Score** | | | **1.004** |

Every program stopped at the same tick with the same output. v1 was not measured: with no gain on v2 there is nothing to re-baseline.

**Verdict.** Rejected: no gain (seven of ten programs slightly slower; the 1.149 of day 3 part 2 is the old build's one slow run, 10.10 against about 10.9 for its neighbours). The hot loop is one function in one crate that chases indices through memory; there is nothing across crate borders to inline and nothing for wider vector instructions to work on. The four settings were tried together only; worth trying again after the flat arrays (attempt 3) and input counting (attempt 4), when the loop is plain arithmetic. The code is on the local tag `v2-attempt-02-rejected`.

### 3. The wiring in flat arrays (2026-11-19): accepted
**Idea.** The netlist keeps the readers, writers, reads and writes of every entity as a list of its own, each in its own block of memory, and the kind as an enum with the ink inside. v2 now copies each of the four into one array with an offset per entity, and keeps what an entity does as one byte.

**Expected.** 1.1 to 1.3: `propagate` and `evaluate` follow one pointer less per list, and neighbouring entities share cache lines.

**Changed.** In `sim2.rs`: a `Lists` type (offsets and items), an `Op` byte per entity, built once in `Engine::new`; `evaluate`, `propagate` and `step` use them. The netlist itself is untouched and still serves the pins and start-up.

**Measured.** Quick run (one run per program, 20 million ticks). Old = attempt 1, new = this attempt, both `--engine v2`.

| Program | Old, M ticks/s | New | Ratio |
|---|---|---|---|
| day1/part1 | 10.46 | 11.49 | 1.099 |
| day1/part2 | 10.26 | 10.72 | 1.045 |
| day2/part1 | 10.69 | 11.35 | 1.062 |
| day2/part2 | 10.29 | 11.26 | 1.095 |
| day3/part1 | 11.07 | 12.27 | 1.108 |
| day3/part2 | 10.80 | 11.54 | 1.068 |
| day4/part1 | 10.50 | 11.14 | 1.061 |
| day4/part2 | 10.16 | 11.24 | 1.106 |
| sample-primes | 9.10 | 10.17 | 1.119 |
| sample-compact-primes | 21.07 | 23.04 | 1.093 |
| **Score** | | | **1.085** |

Start-up unchanged. Every program stopped at the same tick with the same output.

**Equality.** `cargo test` and `cargo test --release --test v2 -- --include-ignored` (2 passed).

**Verdict.** Accepted: 1.085, every program faster (1.045 to 1.119). Just under the estimate's lower end. Bounds checks are still in place; removing them with unsafe indexing is a possible later attempt.

### 4. Input counting (2026-11-19): accepted
**Idea.** A component no longer counts its inputs when it is evaluated. Each component keeps the number of traces it reads that are on, and each trace the number of its writers that are on; the numbers are adjusted when a writer or a trace changes.

**Expected.** 1.2 to 1.4: the 27 input reads per tick and the writer scans go away.

**Changed.** In `sim2.rs`: one `count` per entity, filled at tick 0. `propagate` first adjusts the writer counts of every flipped component and then looks at the traces, in v1's order. The two passes matter: a trace that loses one writer and gains another in the same tick must not change, and must not count as risen. `evaluate` compares the count with the number of inputs. A latch still looks at its inputs to see which rose.

**Measured.** The quick run gave 1.044 with one program at 0.873, too close to the bar to decide. Repeated with three runs per program, 20 million ticks, best of three. Old = attempt 3, new = this attempt, both `--engine v2`.

| Program | Old, M ticks/s | New | Ratio |
|---|---|---|---|
| day1/part1 | 10.85 | 11.36 | 1.047 |
| day1/part2 | 10.91 | 11.71 | 1.073 |
| day2/part1 | 10.98 | 11.83 | 1.078 |
| day2/part2 | 11.05 | 11.72 | 1.060 |
| day3/part1 | 12.83 | 13.37 | 1.042 |
| day3/part2 | 12.25 | 12.95 | 1.057 |
| day4/part1 | 12.24 | 12.71 | 1.039 |
| day4/part2 | 11.93 | 12.74 | 1.068 |
| sample-primes | 10.44 | 11.74 | 1.125 |
| sample-compact-primes | 23.84 | 26.95 | 1.131 |
| **Score** | | | **1.072** |

Start-up unchanged. Every program stopped at the same tick with the same output.

**Equality.** `cargo test` and `cargo test --release --test v2 -- --include-ignored` (2 passed).

**Verdict.** Accepted: 1.072, every program faster. Well under the estimate: the reads that went away are paid for by a count update for every reader of every trace that changes (about 20 per tick). The larger use of the counts is attempt 5, which can now tell from a count that a gate's output cannot change and leave it asleep.

### 5. Gates that cannot change are not woken (2026-11-19): accepted
**Idea.** When a trace changes, v1 queues every component that reads it. Half of those evaluations flip nothing: an AND with another input off, an OR with another input on. With the counts of attempt 4 this is known when the trace changes, so such a gate is not queued.

**Expected.** 1.05 to 1.2.

**Changed.** In `propagate` of `sim2.rs`, with one `threshold` per gate (1 for OR, NOR, Buffer, LED, Breakpoint; the number of inputs for AND and NAND): a gate is queued only when its count reaches or leaves the threshold. A latch or pin is queued only by a rising input. Clocks and timers are never queued. XOR, XNOR and Random are always queued (a Random draws at every change of an input, so its draws stay in v1's order).

Why it is exact: a gate that is not queued has the output its count gives, and an output that depends on the count passing one value can only change when the count passes it. Two changes in one tick that cancel out queue the gate at the first of them, and the evaluation then finds nothing to flip, as in v1.

**Measured.** Quick run (one run per program, 20 million ticks). Old = attempt 4, new = this attempt, both `--engine v2`.

| Program | Old, M ticks/s | New | Ratio |
|---|---|---|---|
| day1/part1 | 11.70 | 12.81 | 1.095 |
| day1/part2 | 11.44 | 12.86 | 1.125 |
| day2/part1 | 12.05 | 13.35 | 1.108 |
| day2/part2 | 11.95 | 13.38 | 1.120 |
| day3/part1 | 13.34 | 15.64 | 1.172 |
| day3/part2 | 13.13 | 14.28 | 1.087 |
| day4/part1 | 12.18 | 14.77 | 1.213 |
| day4/part2 | 11.84 | 14.02 | 1.184 |
| sample-primes | 11.44 | 15.03 | 1.313 |
| sample-compact-primes | 26.32 | 31.80 | 1.208 |
| **Score** | | | **1.16** |

Start-up unchanged. Every program stopped at the same tick with the same output.

**Equality.** `cargo test` and `cargo test --release --test v2 -- --include-ignored` (2 passed): every signal equal at every tick, although v2 now evaluates fewer components than v1.

**Verdict.** Accepted: 1.16, every program faster (1.09 to 1.31), inside the estimate.

### 6. Dropping what cannot cause an event (2026-11-19): accepted
**Idea.** An event is a VMem load or write or a Breakpoint. A component that cannot reach a VMem pin or a Breakpoint through any chain of traces and components cannot cause one, so v2 does not simulate it. A Random is kept with everything that feeds it, because each of its evaluations uses up a draw and so changes what the other Randoms give.

**Expected.** 1.03 to 1.05: the profile only counted the LEDs (4% of the evaluations).

**Changed.** In `Engine::new` of `sim2.rs`: a walk backwards from the pins, Breakpoints and Randoms (a component needs the traces it reads, a trace needs its writers). Components outside it get the operation `Dead`: never queued, never on a clock's list. `live()` says which entities are simulated; `state()` is meaningful only for those. The equality tests compare every simulated signal at every tick, and all of VMem and the Breakpoint as before. `engine/examples/live.rs` prints the share for a board.

| Board | Entities | Simulated by v2 | Dropped |
|---|---|---|---|
| the user's CPU | 10,593 | 3,279 | 69.0% |
| sample | 9,113 | 5,124 | 43.8% |
| sample compact | 6,465 | 3,141 | 51.4% |

**Measured.** Quick run (one run per program, 20 million ticks). Old = attempt 5, new = this attempt, both `--engine v2`.

| Program | Old, M ticks/s | New | Ratio |
|---|---|---|---|
| day1/part1 | 12.01 | 17.45 | 1.453 |
| day1/part2 | 12.60 | 17.81 | 1.413 |
| day2/part1 | 13.17 | 18.92 | 1.437 |
| day2/part2 | 12.74 | 18.73 | 1.470 |
| day3/part1 | 14.81 | 21.01 | 1.418 |
| day3/part2 | 13.60 | 20.02 | 1.472 |
| day4/part1 | 13.73 | 19.29 | 1.405 |
| day4/part2 | 11.76 | 17.79 | 1.512 |
| sample-primes | 14.61 | 14.64 | 1.002 |
| sample-compact-primes | 30.35 | 30.63 | 1.009 |
| **Score** | | | **1.35** |

Start-up unchanged. Every program stopped at the same tick with the same output. A full run of day 4 part 2 on v2: tick 1,339,421,264, answer 9182, 71.4 s (18.7 million ticks per second; 109 s before this attempt).

**Equality.** `cargo test` (71 passed) and `cargo test --release --test v2 -- --include-ignored` (2 passed).

**Verdict.** Accepted: 1.35, and 1.4 to 1.5 on the user's CPU, far above the estimate. The profile counted LEDs only; on the user's CPU a large part of the logic that works every tick ends in LEDs or nowhere and never reaches the memory pins or the Breakpoint. On the two sample boards what is dropped is logic that was idle anyway (the display and input side, with input removed from their program), so nothing is gained there. Not looked at: which parts of the user's CPU these are.

### 7. The event primitive and idle-tick jumps (2026-11-19): accepted
**Idea.** Two things that only work together. (a) The event primitive owed from Scaffolding: the engine advances to the next tick with a VMem load, a VMem write or a Breakpoint, or to a given tick, and the run loop only looks at those stops. (b) While nothing is queued, no trace is still marked as risen and VMem is at rest, a tick changes nothing but the counter, so v2 goes straight to the tick of the next clock or timer flip.

**Expected.** Nothing on the user's CPU (its clock flips every tick), up to 2 on the compact sample (clock interval 36), about 1.05 on the score.

**Changed.**
- `sim.rs`: `advance(until)` on the trait, with a default that steps tick by tick (v1 uses it unchanged); `run_events`, the run loop built on it; the stop rules moved into one function that both run loops call. Ctrl-C is looked at every 2^20 ticks when no event comes.
- `sim2.rs`: v2's own `advance` with the jump.
- `main.rs`: v2 runs by events. v1 keeps the per-tick loop, which `--probe` and `--render` need.
- `tests/v2.rs`: a test that moves v2 by events with limits falling inside idle stretches (also with the clock interval set to 36) against v1 stepped tick by tick: v2 may stop only at an event or its limit, may pass over no event of v1, and must be equal at every stop.

**Measured.** The quick run gave 1.019 (the compact sample 1.23, the others between 0.96 and 1.05), under the bar, so it was repeated with three runs per program, 20 million ticks, best of three. Old = attempt 6, new = this attempt, both `--engine v2`.

| Program | Old, M ticks/s | New | Ratio |
|---|---|---|---|
| day1/part1 | 18.74 | 19.32 | 1.031 |
| day1/part2 | 19.19 | 19.70 | 1.027 |
| day2/part1 | 19.55 | 20.68 | 1.058 |
| day2/part2 | 20.06 | 17.48 | 0.872 |
| day3/part1 | 21.12 | 22.20 | 1.051 |
| day3/part2 | 20.94 | 21.62 | 1.032 |
| day4/part1 | 20.49 | 21.30 | 1.039 |
| day4/part2 | 19.01 | 21.23 | 1.117 |
| sample-primes | 14.67 | 14.91 | 1.016 |
| sample-compact-primes | 31.15 | 39.22 | 1.259 |
| **Score** | | | **1.046** |

Day 2 part 2's 0.872 is noise: its old build spread 8.3% in this run, and it was 1.001 in the quick run. Start-up unchanged. Every program stopped at the same tick with the same output; `--verbose` output of v1 and v2 is byte for byte the same on day 2 part 1 (to its Breakpoint) and on the compact sample (30 million ticks), and a `--quiescent` stop falls on the same tick.

**Equality.** `cargo test` and `cargo test --release -- --ignored`.

**Verdict.** Accepted: 1.046. The compact sample gains a quarter, less than the "up to 2" its idle ticks suggested: an idle tick was already cheap. The user's CPU gains 3% to 5% although it has no idle ticks, because the run loop no longer runs its checks after every tick. Kept in any case as groundwork: replay (attempt 8) needs the event primitive.

### 8. Replay of repeated steps (2026-11-19): accepted
**Idea.** A **node** is the circuit's full state at the end of a tick with an event. From a node the circuit is deterministic up to the next event, so the number of ticks to it and what it is (load address, write address, Breakpoint) are stored once. If that event is a load, the state after it also depends on the word loaded, so there is one successor per word seen. Standing at a known node whose step is known, v2 moves to the next event without the circuit: it adds the ticks, reports the event, applies the write. When it meets something new it puts the circuit into the node's state and simulates.

**Expected.** 1.5 to 3 on top of stage 1. Measured before building, with `engine/examples/replay_estimate.rs`: the share of ticks inside steps seen before was 46% (day 1 part 1), 83% (day 2 part 2), 60% (day 4 part 2, first 100 million ticks), 89% and 92% (the two samples).

**Changed.**
- `engine/src/replay.rs` (new): the table. Nodes are found by a hash and reused only after their whole stored state has been compared.
- `sim2.rs`: `advance` tries a replayed step first; in simulation it looks up or stores the node at every event and records the step. `sync()` brings the circuit to the current tick (tests use it; `state()` is otherwise that of an earlier node while replay runs). `set_memory_limit`, `replay_stats`.
- A stop that falls inside a known step is never replayed: the step is simulated tick by tick, as the spec demands.
- Memory: the table stays under 2 GiB (590 bytes per node on the user's CPU). At the limit no new node is stored and everything known keeps working.
- Off on a board with Random ink (every evaluation uses up a draw) and on a board with a simulated Timer (its position would be part of every state). The compact sample has such timers, so it runs as before.

**What a node holds,** and where this differs from the spec's list: one bit per simulated signal; the address the pins hold, the VMem lock and a pending load; the position within the clock interval; and, in place of the queue and the just-risen marks, **the components that flip in the next tick**. That is all the queue and the marks are for, and it is known at the moment the node is taken. Two states that behave alike are then one node. The words VMem delivers are not in the node: every load ends a step, and the word selects the successor.

**What went wrong on the way,** each found by measuring:
1. First version: 1.50 on the quick run, with day 1 slower than before. Packing the signals for each lookup wrote to memory once per signal; built a word at a time in a register it went from 40% of the time to a few percent.
2. Restoring a node recomputed every count. It now flips only the signals that differ from the node the arrays hold and adjusts the counts they feed.
3. The hit rate stayed far under the estimate (12% against 46% on day 1 part 2). Neither sorting the lists nor replacing them by the next tick's flips changed the node count at all. The cause was in the measuring tool: it gave the engine a timer interval of 1000, and the tick's position in it was part of every node. On a board without a simulated timer that position is now left out. The next-tick-flips form was kept: it is the smaller and more general key.

**Measured.** As the rule allows for an attempt that needs warm-up: every program to its Breakpoint, one run, old = attempt 7, new = this attempt, both `--engine v2`. (The quick run of 20 million ticks gave 2.24.)

| Program | Ticks | Old, M ticks/s | New | Ratio |
|---|---|---|---|---|
| day1/part1 | 43,146,512 | 20.74 | 32.61 | 1.57 |
| day1/part2 | 70,339,472 | 21.07 | 32.27 | 1.53 |
| day2/part1 | 43,495,312 | 22.09 | 76.04 | 3.44 |
| day2/part2 | 75,753,616 | 22.07 | 98.90 | 4.48 |
| day3/part1 | 170,904,144 | 24.06 | 68.91 | 2.86 |
| day3/part2 | 419,669,712 | 21.36 | 59.17 | 2.77 |
| day4/part1 | 481,327,888 | 21.54 | 44.22 | 2.05 |
| day4/part2 | 1,339,421,264 | 21.07 | 47.86 | 2.27 |
| sample-primes | 100,000,000 (limit) | 14.25 | 164.74 | 11.57 |
| sample-compact-primes | 100,000,000 (limit) | 40.60 | 40.78 | 1.00 |
| **Score** | | | | **2.63** |

Start-up unchanged (0.06 to 0.08 s). Every program stopped at the same tick with the same output. Day 4 part 2 alone: 29.0 s, answer 9182, peak memory 670 MB.

**Equality.** `cargo test` (72 passed) and `cargo test --release -- --ignored` (4 passed), among them a new one: day 1 part 1 to its Breakpoint with v2 moved by events and limits of many sizes, against v1 tick by tick, once with all the memory and once with 3 MiB for the table. `--verbose` output of v1 and v2 is byte for byte the same on day 2 part 2.

**Verdict.** Accepted: 2.63 on full runs, inside the estimate. The share replayed, and so the gain, depends on how many different register contents a program passes through: day 2 and the sample repeat most, day 1 least.

### 9. Chaining replays (2026-11-19): rejected
**Idea.** Every replayed step ends in an event, and `advance` returns to the run loop for each. When nobody needs to see the VMem events one by one (no `--verbose`, no address watch, no `--quiescent`), v2 could run on through them and return only at a Breakpoint or its limit.

**Expected.** Little. A replayed step is a few table lookups; on the best case (day 2 part 2, 83% replayed) the simulated 17% of the ticks already take about four fifths of the time.

**Changed.** A `chain` switch on v2's engine, set by `main.rs` from the options; two conditions in `advance`.

**Measured.** Quick run (one run per program, 20 million ticks). Old = attempt 8, new = this attempt, both `--engine v2`.

| Program | Old, M ticks/s | New | Ratio |
|---|---|---|---|
| day1/part1 | 29.11 | 29.20 | 1.003 |
| day1/part2 | 26.74 | 26.46 | 0.989 |
| day2/part1 | 75.19 | 62.31 | 0.829 |
| day2/part2 | 104.17 | 110.50 | 1.061 |
| day3/part1 | 63.29 | 62.70 | 0.991 |
| day3/part2 | 51.68 | 51.68 | 1.000 |
| day4/part1 | 42.92 | 42.55 | 0.991 |
| day4/part2 | 34.13 | 33.50 | 0.982 |
| sample-primes | 88.89 | 88.50 | 0.996 |
| sample-compact-primes | 39.22 | 38.46 | 0.981 |
| **Score** | | | **0.98** |

Every program stopped at the same tick with the same output.

**Verdict.** Rejected: no gain; eight of ten programs within 2% of 1. The return to the run loop is not where the time is. With replay in place, the time is in the steps that are simulated: the misses. The code is on the local tag `v2-attempt-09-rejected`.

### 10. Netlist: traces with one writer merged into it (2026-11-19): accepted
**Idea.** The user's: make the simulated circuit smaller. Measured first with `engine/examples/netlist_stats.rs` what the simulated part of each board offers:

| | the user's CPU | sample | sample compact |
|---|---|---|---|
| Simulated entities | 3,279 (1,380 traces, 1,867 components, 32 pins) | 5,124 | 3,141 |
| Traces with exactly one writer | 1,168 (85% of the traces) | 1,938 (91%) | 1,024 (79%) |
| Components that duplicate another (same ink, same inputs) | 12 | 331 | 0 |
| Entities that can never change | 37 | 112 | 49 |
| Gates with one input | 90 | 854 | 78 |

A trace with one writer equals that writer in every tick, so it need not exist: whoever read the trace reads the writer. That is the one large item on the user's CPU: 36% of the simulated entities. Duplicates and constants are 1.5% there and were left alone.

**Expected.** 1.2 to 1.4 on the simulated part.

**Changed.** In `Engine::new` of `sim2.rs`: the reads of every component are rewired from such a trace to its writer; each component gets a `direct` list, the readers of the traces only it writes; all lists lose what is not simulated. `propagate` feeds the direct readers straight from the component. A component with direct readers is marked as risen when it turns on, which is what a latch reading it looks for. Not on a board with Random ink, where readers must be woken in v1's order. The merged traces are no longer simulated, so `live()` excludes them and the equality tests compare the components.

Simulated entities after this: 2,111 on the user's CPU (80.1% of the board dropped), and a stored replay state shrinks with them.

**Measured.** The quick run gave 1.07 with single programs between 1.00 and 1.23, so it was repeated with three runs per program, 20 million ticks, best of three. Old = attempt 8, new = this attempt, both `--engine v2`.

| Program | Old, M ticks/s | New | Ratio |
|---|---|---|---|
| day1/part1 | 29.59 | 31.50 | 1.065 |
| day1/part2 | 27.47 | 28.13 | 1.024 |
| day2/part1 | 74.91 | 79.05 | 1.055 |
| day2/part2 | 109.89 | 117.65 | 1.071 |
| day3/part1 | 63.09 | 65.57 | 1.039 |
| day3/part2 | 51.55 | 54.95 | 1.066 |
| day4/part1 | 42.83 | 45.25 | 1.057 |
| day4/part2 | 34.13 | 36.23 | 1.062 |
| sample-primes | 88.89 | 95.24 | 1.071 |
| sample-compact-primes | 39.60 | 39.29 | 0.992 |
| **Score** | | | **1.05** |

Every program stopped at the same tick with the same output. Day 4 part 2 to its Breakpoint: 28.2 s (29.0 s before).

**Equality.** `cargo test` (72 passed) and `cargo test --release -- --ignored` (4 passed).

**Verdict.** Accepted: 1.05. A third fewer entities gave a twentieth more speed. The size of the circuit is not what costs time: an entity that does not change costs nothing in an event-driven engine. What costs is each change, and a merged trace saved only part of the work of one change (one count, one comparison, one store). To gain more from the netlist it would have to make fewer things change per tick, not fewer things exist.

### 11. The self-running part played from a schedule (2026-11-19): accepted
**Idea.** Part of a board runs by itself: everything a clock drives that no VMem pin, Random or Timer can ever influence. Its behaviour is fixed from tick 0, whatever the program does, and it soon repeats. v2 finds that part in the netlist, runs it once before the simulation until it is in a situation it was in before, and from then on only plays the flips it hands to the rest of the circuit. This is the first step of the user's part-level skipping: the part without inputs.

It was found by looking at where the activity is (`engine/examples/heatmap.rs`), but the rule uses nothing of what was seen there: it is a reachability rule on the netlist (`engine/examples/autonomous.rs` shows what it finds).

| | the user's CPU | sample | sample compact |
|---|---|---|---|
| Entities in the self-running part | 23 | 156 | few |
| Period | 64 ticks | 128 ticks | |
| Share of all simulated flips inside it | 58% | 60% | 0.8% |
| Traces the rest of the circuit reads from it | 2 | 3 | |

**Expected.** About 2 on the simulated steps: 60% of the flips go, and with the clock no longer flipping every tick the idle-tick jump (attempt 7) starts to work on boards with a clock interval of 1.

**Changed.** In `sim2.rs`: `Schedule::find` (two walks through the netlist, then the reference engine run until the part's situation repeats: its signals now and one tick ago and the position in the clock interval, at most 65,536 ticks); the part's entities that the rest reads are kept and flipped from the schedule, the others are not simulated; `step_real` adds the tick's scheduled flips; the idle jump goes to the next scheduled flip; a replay node holds the position in the schedule where it held the clock's position. Off on a board with Random ink. A part that does not repeat within the limit is simulated as before.

**Measured.** Quick run (one run per program, 20 million ticks). Old = attempt 10, new = this attempt, both `--engine v2`.

| Program | Old, M ticks/s | New | Ratio |
|---|---|---|---|
| day1/part1 | 29.59 | 49.75 | 1.68 |
| day1/part2 | 27.47 | 45.98 | 1.67 |
| day2/part1 | 77.22 | 119.05 | 1.54 |
| day2/part2 | 106.95 | 180.18 | 1.69 |
| day3/part1 | 61.73 | 112.36 | 1.82 |
| day3/part2 | 51.41 | 93.46 | 1.82 |
| day4/part1 | 43.57 | 76.34 | 1.75 |
| day4/part2 | 34.01 | 58.65 | 1.72 |
| sample-primes | 87.72 | 192.31 | 2.19 |
| sample-compact-primes | 40.73 | 38.68 | 0.95 |
| **Score** | | | **1.65** |

Start-up unchanged (the pre-run is a few hundred ticks). Every program stopped at the same tick with the same output. Day 4 part 2 to its Breakpoint: 18.2 s, answer 9182 (28.2 s before; about 3.6 minutes on v1).

**Equality.** `cargo test` (72 passed) and `cargo test --release -- --ignored` (4 passed). `--verbose` output of v1 and v2 is byte for byte the same on 30 million ticks of the sample computer.

**Verdict.** Accepted: 1.65; 1.5 to 1.8 on the user's CPU and 2.2 on the sample. The compact sample has almost no self-running part and loses 5% in this one run (not repeated; one extra check per tick is the likely cost).

### 12. Part-level skipping: units with their own tables (2026-11-19): rejected
**Idea.** The user's, in the form the estimates supported (see "Ideas not tried"): sparsely connected parts of the circuit, found from the netlist alone, each skipped by itself when it recognises its situation. Built as step 1 of two: the mechanism alone, with whole-board replay switched off, so that its real cost could be measured against the plain simulation before the two were joined and before any switching of units on and off.

**How it worked.** A **unit** is a part's gates, latches and inner traces. It has a table of nodes: its signals, its inputs' values, and its components that flip in the next tick. From a node the next one follows from which inputs flip in the tick between: none (one array lookup) or a set of them (one hash lookup). While a unit is **frozen** its inside is not simulated: each tick its node says which of its outputs flip, and the next node is looked up. When a lookup fails the unit is **thawed**: its signals are rebuilt from the node, it catches up with the tick it missed, and it is simulated like everything else, which also teaches the table. Units were chosen at start-up: cut the traces more than 32, 16, 8, 4, 2 components share, split a part while it has more than `largest` entities, take it if it has at least 32.

**Equality.** A test stepped v2 with units against v1 on both day 1 programs, tick by tick and by events with limits of many sizes, for 60,000 ticks at two unit sizes: every simulated signal, every VMem event and the Breakpoint equal. A self-check (a unit at rest must show exactly its node's signals) ran clean. One real fault was found on the way by the engine's own refusal to store two outcomes for one situation: the set of flipped inputs was hashed as a sum of multiples of the entity numbers, and inputs 246 and 252 flipping gave the same sum as 248 and 250. The whole-program test was not run.

**Measured.** 20 million ticks, whole-board replay off in every row, so the first row of each block is the plain simulation the others are compared with.

| Program | `largest` | Units | Flips skipped per tick | Lookups failed per tick | M ticks/s | Against plain |
|---|---|---|---|---|---|---|
| day 1 part 2 | none | 0 | | | 21.6 | |
| | 128 | 2 | 0.15 | 0.0013 | 20.3 | 0.94 |
| | 256 | 6 | 0.55 | 0.0037 | 16.5 | 0.76 |
| | 512 | 7 | 0.97 | 0.0049 | 17.2 | 0.80 |
| | 4096 | 1 | 0.65 | 0.0003 | 2.9 | 0.13 |
| day 4 part 2 | none | 0 | | | 21.2 | |
| | 128 | 2 | 0.15 | 0.0011 | 20.9 | 0.98 |
| | 256 | 6 | 0.45 | 0.0032 | 19.9 | 0.94 |
| | 512 | 7 | 0.68 | 0.0043 | 19.6 | 0.92 |
| | 4096 | 1 | 0.58 | 0.0004 | 3.0 | 0.14 |

**Verdict.** Rejected: slower at every setting, although the units do skip up to a third of the flips that are left. Three reasons, all measured:
- **There is little left to skip.** After attempts 5, 6, 10 and 11 the simulated circuit makes about 3 flips per tick, and a tick of the plain simulation costs about 47 ns, most of it not in the flips. Skipping one flip per tick saves roughly 10 ns.
- **A failed lookup is dear.** Thawing rebuilds the unit's counts and evaluates every member once: about 1 to 2 microseconds for a unit of a few hundred entities. At 0.004 to 0.005 failures per tick that is 5 to 10 ns per tick, as much as the skipping saves.
- **Learning is dear.** While a unit is simulated in a burst, its node is looked up or stored every tick (0.01 to 0.03 per tick), each time packing its signals.
- The one large unit loses outright: it fills its table (a million nodes) and pays for a lookup of 3,000 signals in most ticks.

The estimates in "Ideas not tried" counted flips saved against lookups; they did not count that a flip is only a small part of what a tick costs now. With the best possible choice of units they promised 0.45 to 0.82 flips per tick, which is 5 to 8 ns of 47: the gain was never going to be large, and the machinery costs more than that. Step 2 (joining with whole-board replay, switching units on and off while running) was not built: it could only choose among units that each lose. The code is on the local tag `v2-attempt-12-rejected`; the estimator and the tree tools stay in `engine/examples/`.

What this points to: the time left in a missed step is in the tick itself (about 16 ns per flip against about 47 ns per tick), so the next gains are in the cost of a tick, not in skipping more flips.

### 13. No fixed work in a tick (2026-11-19): accepted
**Idea.** Attempt 12 showed that a simulated tick costs about 47 ns of which the flips are a small part. Reading the tick against that number: every tick divided three times (is it a clock tick, a timer tick, where in the schedule), scanned its list of flips twice for VMem pins, and went through the VMem section whether or not a pin had moved.

**Expected.** Unknown; a few percent to ten.

**Changed.** In `sim2.rs`: the next tick at which the clocks, the timers and the schedule flip something is kept (`clock_due`, `timer_due`, `schedule_due`) and compared, not computed; `set_due` renews the three when replay has moved the tick counter. The apply loop notes which kinds of VMem pin flipped, and the VMem section is entered only when one did, a load is pending or VMem is locked. The idle-tick jump uses the three kept ticks. One slip from attempt 11 was found by the equality tests and fixed: the schedule's table of "ticks until the next flip" had no entry for tick 0 (it had not mattered, because nothing asked at tick 0).

**Measured.** The quick run gave 1.051, close to the bar; repeated with three runs per program, 20 million ticks, best of three. Old = attempt 11, new = this attempt, both `--engine v2`.

| Program | Old, M ticks/s | New | Ratio |
|---|---|---|---|
| day1/part1 | 48.78 | 50.51 | 1.035 |
| day1/part2 | 44.84 | 48.31 | 1.077 |
| day2/part1 | 129.03 | 132.45 | 1.026 |
| day2/part2 | 181.82 | 200.00 | 1.100 |
| day3/part1 | 112.36 | 124.22 | 1.106 |
| day3/part2 | 90.91 | 99.50 | 1.095 |
| day4/part1 | 74.63 | 78.74 | 1.055 |
| day4/part2 | 56.34 | 62.11 | 1.102 |
| sample-primes | 206.19 | 192.31 | 0.933 |
| sample-compact-primes | 40.73 | 42.19 | 1.036 |
| **Score** | | | **1.055** |

The sample's 0.933 is within its spread in this run (4.9% and 4.6%) and was 1.000 in the quick run. Every program stopped at the same tick with the same output.

**Equality.** `cargo test` and `cargo test --release -- --ignored`.

**Verdict.** Accepted: 1.055 on the first 20 million ticks. On a whole run it is less: day 4 part 2 to its Breakpoint took 18.1 s against 18.4 s before, 2%.

### 14. Indexing without bounds checks in the tick (2026-11-19): accepted
**Idea.** Every array access in the tick's inner loops checked its index. All those indices are entity numbers or positions from the netlist's own lists, made in `Engine::new` for arrays of exactly that size.

**Expected.** A few percent.

**Changed.** In `sim2.rs`: two helpers, `g` and `gm`, that index without the check in a release build and with it in a debug build; used in `evaluate`, `propagate`, `feed`, the evaluation and apply loops of `step_real`, and `Lists::of`. Nothing else is unsafe. The spec allows unsafe code; what guards it is that `cargo test` (a debug build, with every check on) runs the equality tests over the same code.

**Measured.** Quick run (one run per program, 20 million ticks). Old = attempt 13, new = this attempt, both `--engine v2`.

| Program | Old, M ticks/s | New | Ratio |
|---|---|---|---|
| day1/part1 | 54.64 | 63.69 | 1.166 |
| day1/part2 | 51.28 | 58.48 | 1.140 |
| day2/part1 | 142.86 | 160.00 | 1.120 |
| day2/part2 | 210.53 | 232.56 | 1.105 |
| day3/part1 | 129.87 | 144.93 | 1.116 |
| day3/part2 | 109.29 | 120.48 | 1.102 |
| day4/part1 | 85.84 | 95.69 | 1.115 |
| day4/part2 | 67.34 | 74.91 | 1.112 |
| sample-primes | 229.89 | 277.78 | 1.208 |
| sample-compact-primes | 44.25 | 54.95 | 1.242 |
| **Score** | | | **1.14** |

Every program stopped at the same tick with the same output.

**Equality.** `cargo test` (debug, indices checked) and `cargo test --release -- --ignored`.

**Verdict.** Accepted: 1.14, every program faster (1.10 to 1.24). On a whole run it is more: day 4 part 2 to its Breakpoint took 13.9 s against 18.1 s before, 1.30. More than expected: the checks also kept the compiler from holding values in registers across the loops.

### 15. Compiler settings, tried again (2026-11-19): accepted
**Idea.** Attempt 2 with the engine as it is now: `lto = "fat"`, `codegen-units = 1`, `panic = "abort"` in the release profile and `-C target-cpu=native`. Rejected then at 1.004, with the note that it was worth trying again once the loop was plain arithmetic.

**Expected.** A few percent.

**Changed.** Six lines in `engine/Cargo.toml` and `engine/.cargo/config.toml`, the same as on the tag `v2-attempt-02-rejected`. `cargo build --release` with the documented environment produces the accepted build, as the spec requires. What it costs: a release build takes 9 seconds where it took 4; the binary is built for this machine's CPU and may not run on an older one; a panic ends the program at once, without unwinding.

**Measured.** The quick run gave 1.039, at the bar; repeated with three runs per program, 20 million ticks, best of three. Old = attempt 14, new = this attempt.

| Program | v2 old, M ticks/s | v2 new | Ratio | v1 ratio (one run) |
|---|---|---|---|---|
| day1/part1 | 62.31 | 64.94 | 1.042 | 1.004 |
| day1/part2 | 56.98 | 59.35 | 1.042 | 0.980 |
| day2/part1 | 158.73 | 163.93 | 1.033 | 0.974 |
| day2/part2 | 227.27 | 240.96 | 1.060 | 1.001 |
| day3/part1 | 140.85 | 149.25 | 1.060 | 0.999 |
| day3/part2 | 116.28 | 122.70 | 1.055 | 0.992 |
| day4/part1 | 97.56 | 98.04 | 1.005 | 0.997 |
| day4/part2 | 74.07 | 75.47 | 1.019 | 1.001 |
| sample-primes | 277.78 | 285.71 | 1.029 | 0.981 |
| sample-compact-primes | 55.10 | 56.98 | 1.034 | 1.014 |
| **Score** | | | **1.038** | **0.994** |

Every program stopped at the same tick with the same output. Day 4 part 2 to its Breakpoint: 13.8 s against 14.2 s for the old build in the same session.

The settings apply to both engines. v1 does not change (0.994, inside the noise), so its baseline stands: about 6.2 to 6.7 million ticks per second on the user's CPU.

**Equality.** `cargo test` and `cargo test --release -- --ignored`, built with the new settings.

**Verdict.** Accepted: 1.038 with every program at or above 1, over the bar of 1.03 and of twice the noise (1%). A small gain for a build that is slower to make and tied to this machine's CPU; if the binary ever has to run elsewhere, `.cargo/config.toml` is the file to remove.

### v2.1, steps 2 and 3 (2026-11-19): attempts 16 to 19
From here on the yardstick is v2 itself; v1 is only the reference for equality. The plan and the profile these attempts follow are in `vcb-engine-v2.1.md`. Old = the engine at attempt 15 with v2 as the default; each attempt is measured against the last accepted build. 20 million ticks per program.

**16. The rare parts of a tick as cold functions: rejected at first, then accepted (see below).** The VMem section and a Random's draw moved into functions marked cold, and the Breakpoint test taken out of the loop that applies the flips (a flag per entity). Quick run 1.030; three runs 1.015, with two programs slower. Under the bar. (ESSENT's gain from hints came from a hot code footprint larger than the cache; this loop already fits.) Tag `v2-attempt-16-rejected`.

**17. Replay keyed on the loaded word's content bits only: rejected.** The word that selects a step's successor masked to the number of content pins. Quick run 1.000: on these boards the programs do not hold words wider than the pins. Tag `v2-attempt-17-rejected`. (The first measurement of this attempt compared two builds of attempt 16 by mistake, after a failed build left the old binary in place, and showed 1.036: a reminder of what one quick run can be off by. The script now stops when a build fails.)

**18. A latch keeps a toggle flag; no just-risen marks: accepted, 1.045.** v1's rule is that a latch toggles once per input that rose, so v2 marked every signal that rose in a tick, cleared the marks in the next, and a latch looked through its inputs for marks. Now a rising input flips a flag on each latch it feeds (it queues the latch at the same moment), and the latch's evaluation is its state against the flag. The marks, their list and their clearing are gone from every flip. Equal to v1 on every test.

| Program | Old, M ticks/s | New | Ratio |
|---|---|---|---|
| day1/part1 | 65.36 | 68.49 | 1.048 |
| day1/part2 | 60.42 | 61.16 | 1.012 |
| day2/part1 | 165.29 | 173.91 | 1.052 |
| day2/part2 | 243.90 | 256.41 | 1.051 |
| day3/part1 | 149.25 | 152.67 | 1.023 |
| day3/part2 | 116.28 | 125.00 | 1.075 |
| day4/part1 | 99.50 | 103.09 | 1.036 |
| day4/part2 | 76.34 | 79.68 | 1.044 |
| sample-primes | 277.78 | 285.71 | 1.029 |
| sample-compact-primes | 55.87 | 60.24 | 1.078 |
| **Score** | | | **1.045** |

**19. The packed state kept up to date at every change: accepted, 1.11.** The profile of step 0 showed 13% of the time in packing 2,100 signals into bits at every event of a simulated step, to look the state up. Now every change of a simulated signal flips its bit in a packed copy, and the lookup uses the copy as it is. A board without replay does not keep the copy (the first version made the compact sample 5% slower for nothing). Old = attempt 18.

| Program | Old, M ticks/s | New | Ratio |
|---|---|---|---|
| day1/part1 | 62.89 | 71.17 | 1.132 |
| day1/part2 | 58.48 | 66.67 | 1.140 |
| day2/part1 | 165.29 | 183.49 | 1.110 |
| day2/part2 | 240.96 | 270.27 | 1.122 |
| day3/part1 | 145.99 | 168.07 | 1.151 |
| day3/part2 | 123.46 | 141.84 | 1.149 |
| day4/part1 | 98.52 | 111.11 | 1.128 |
| day4/part2 | 73.53 | 86.21 | 1.172 |
| sample-primes | 273.97 | 281.69 | 1.028 |
| sample-compact-primes | 57.64 | 56.82 | 0.986 |
| **Score** | | | **1.11** |

**16 again, accepted.** The user lowered the bar to 1.015 so that attempt 16 counts. Put back on top of attempts 18 and 19 and measured again, three runs, old = attempt 19:

| Program | Old, M ticks/s | New | Ratio |
|---|---|---|---|
| day1/part1 | 74.63 | 77.22 | 1.035 |
| day1/part2 | 70.18 | 72.99 | 1.040 |
| day2/part1 | 188.68 | 192.31 | 1.019 |
| day2/part2 | 281.69 | 285.71 | 1.014 |
| day3/part1 | 173.91 | 176.99 | 1.018 |
| day3/part2 | 144.93 | 148.15 | 1.022 |
| day4/part1 | 115.61 | 121.21 | 1.048 |
| day4/part2 | 89.69 | 94.34 | 1.052 |
| sample-primes | 317.46 | 327.87 | 1.033 |
| sample-compact-primes | 57.97 | 60.79 | 1.049 |
| **Score** | | | **1.033** |

Every program faster, and over the old bar too: with the just-risen marks and the packing gone, the rare code is a larger share of what is left in the tick. Equal to v1 on every test. The tag `v2-attempt-16-rejected` keeps the first version.

**Not tried, with the reason:**
- *The event loop's own overhead:* 2.5% of the time in the profile taken after attempt 19; nothing to win.
- *Fewer lookups during a run of misses:* after attempt 19 looking a state up and storing it is about 2% of the time.
- *Profile-guided optimisation:* needs the toolchain's `llvm-tools` component, which is not installed; left for the user to decide.

**Both together, on full runs** (every program to its end, one run; old = attempt 15): 1.17. Day 4 part 2 runs at 116 M ticks per second, 11.5 s. `cargo test` and `cargo test --release -- --ignored` pass.

**The profile after attempt 19** (day 4 part 2): `propagate` 60% of the time, the rest of the simulated tick 28%, everything to do with replay 12%. The ticks are crossed as before: 63% replayed, 27% idle, 9.5% simulated.

### v2.1, step 4 (2026-11-19): attempts 20 and 21, the layout
After attempt 19 the function that applies flips was 60% of the time, and for every gate a flip feeds it touched four arrays. The bar is 1.015 from here on. Three runs per program, 20 million ticks.

**20. One record per entity: accepted, 1.028.** Count, threshold, operation, state, queued flag and toggle flag, six arrays before, are one record of eight bytes (`Cell`). The counts became 16 bits wide; a board with a gate of more than 65,535 inputs is refused by v2 with a message that names `--engine v1`. `state()` shows a copy of the signals that `sync` and `step` bring up to date, since the signals themselves now live in the records. Old = attempt 16 as accepted.

| Program | Old, M ticks/s | New | Ratio |
|---|---|---|---|
| day1/part1 | 73.53 | 75.47 | 1.026 |
| day1/part2 | 69.93 | 71.17 | 1.018 |
| day2/part1 | 181.82 | 194.17 | 1.068 |
| day2/part2 | 277.78 | 281.69 | 1.014 |
| day3/part1 | 173.91 | 173.91 | 1.000 |
| day3/part2 | 149.25 | 152.67 | 1.023 |
| day4/part1 | 114.94 | 119.76 | 1.042 |
| day4/part2 | 91.32 | 93.46 | 1.023 |
| sample-primes | 307.69 | 322.58 | 1.048 |
| sample-compact-primes | 59.17 | 60.42 | 1.021 |
| **Score** | | | **1.028** |

No program slower. Less than the profile suggested: the four arrays were already in the first cache, so one access in place of four saved instructions, not waiting. (The first build of this attempt copied all signals for `state()` in every pass of the event loop and ran at a tenth of the speed; the benchmark showed it at once.)

**21. The engine's own numbering, simulated entities first: rejected, 0.999.** The 2,100 simulated entities got the numbers 0 to 2,099 inside the engine, so that their records lie together and an entity's number is its bit in the packed state. Equal to v1 on every test; five programs slower, three faster, none by more than 5%. The whole of what a tick touches fitted the first cache before, so moving it closer together changes nothing. 16-bit indices in the lists were not tried: they aim at the same thing. Tag `v2-attempt-21-rejected`.

**After step 4:** day 4 part 2 to its Breakpoint in 11.1 s (121 M ticks per second). `cargo test` and `cargo test --release -- --ignored` pass.

What this step says about the simulated tick: its cost is not memory. The data is small enough to sit in the first cache whatever its layout. What is left is the number of instructions per flip and the branches in them.

### v2.1, step 5 (2026-11-19): attempt 22, the replay table across runs and under a limit
**Emptying the table at its limit.** Until now replay stopped learning when its table reached the memory limit. FastSim's answer is to throw the table away and go on. Measured by giving replay less memory than it wants (whole runs, `engine/examples/replay_stats.rs`):

| Program | Limit | Stops learning (before) | Emptied and learnt again (now) | Unlimited |
|---|---|---|---|---|
| day 4 part 2 | 32 MiB | 17.0 s, 41.6% of ticks replayed | 12.5 s, 57.8% (emptied 42 times) | 11.8 s, 64.8%, 1,142 MiB |
| day 4 part 2 | 128 MiB | 16.5 s, 44.7% | 12.2 s, 62.6% (9 times) | |
| day 3 part 2 | 32 MiB | 2.70 s, 66.0% | 2.77 s, 65.6% (9 times) | 2.70 s, 68.1%, 293 MiB |
| day 3 part 2 | 128 MiB | 2.82 s, 66.7% | 2.79 s, 67.5% (twice) | |

Emptying is better wherever the limit bites, and it shows something about replay: a table of 32 MiB is within 6% of one of 1.1 GiB. States are reused soon after they are learnt, not across long stretches of a run. Emptying at the limit is the engine's behaviour now; `--replay-memory <MiB>` sets the limit (default 2048).

**The table saved between runs.** `--replay-cache <file>`: read before the run, written after (through a temporary file; not rewritten when nothing new was learnt). The file carries a fingerprint of the circuit as the engine holds it, its settings and the engine's own executable, so only the same build on the same board uses it; anything else is ignored with a line saying why, and replaced. Every number is checked on reading, since the tick's loops index without bounds checks: a damaged file is an error, never a wrong answer or a crash.

| Run | First time | Again, with the table | The file |
|---|---|---|---|
| day 2 part 1 | 0.31 s (0.69 s with start-up and saving) | 0.008 s (0.11 s in all) | 24 MiB |
| day 4 part 2 | 11.2 s (12.5 s in all) | 0.81 s (1.28 s in all), checked with `--verify` against v1 | 994 MiB |
| day 2 part 2 with day 2 part 1's table | 0.32 s | (no faster than without: another program passes through other states) | |

**Verdict.** Accepted as a feature. It does nothing for a first run (the benchmark is unchanged, 1.001 over three runs) and makes a repeated run of the same program 10 to 40 times faster. A different program gains little from another's table, in line with the finding above that states are local to where a program is. Equality: `cargo test` (73 passed, one new: the cache written, read back, refused for another board, a file cut short, a spoiled number, a file that is something else, a 1 MiB table) and `cargo test --release -- --ignored`.

## Ideas not tried

**Rule for every idea below** (the user, 2026-11-19): the engine optimises the simulator, not a board. Parts, blocks or schedules are found by the engine in the netlist it is given; nothing is marked by hand or tuned to the user's CPU.

- **Sparsely connected parts, skipped by themselves, recursively** (the user's idea; wanted). A part is steered from outside only through its inputs; when an input changes it starts a burst, and what it then does follows from its own signals, its inputs and what just changed. A burst seen before could be replayed. Measured with `engine/examples/parts_estimate.rs`, with parts found from the netlist alone (cut every trace that more than T components share, T a power of two; 3 million ticks):

  | | Flips in the self-running part | Flips in bursts seen before (all levels together) | Left to simulate |
  |---|---|---|---|
  | the user's CPU, day 4 part 2 | 57.7% | 37.0% | 5.3% |
  | the user's CPU, day 1 part 2 | 52.8% | 40.5% | 6.7% |
  | sample | 60.1% | 38.9% | 1.0% |
  | sample compact | 0.8% | 96.3% | 3.0% |

  So a crude partition is enough for the bursts to repeat. The open side is the cost: every burst needs a lookup, and fine parts have many small bursts. On the user's CPU the coarsest level (5 parts) has 0.04 input events per tick and its known bursts hold 18% of all flips, about 32 flips per lookup; the next level (66 parts) has 0.62 events per tick for 9.6 points more, about 1 flip per lookup, which a lookup cannot beat. The first pictures (`engine/examples/parts.rs`) used thresholds picked by looking at one board and are for understanding only. Asking the user to mark blocks was suggested and is withdrawn: it would break the rule above. The self-running part (attempt 11) was the first step; the next is the coarse level, with a measurement of what a lookup costs against what its burst saves.

  **The level is a setting of the estimator** (its fourth argument), and it was swept on all three boards after attempt 11, counting only what whole-board replay does not already skip (3 million ticks from a cold start). "Per lookup" is flips saved per burst looked up; simulating a flip costs roughly a tenth of a hashed lookup, so a level pays only well above 10.

  | Board, program | Coarsest level: share of the missed flips, per lookup | T=16: share, per lookup | T=4: share, per lookup |
  |---|---|---|---|
  | the user's CPU, day 4 part 2 | 22.9%, 226 | 46.1%, 2.2 | 71.1%, 1.1 |
  | the user's CPU, day 1 part 2 | 4.5%, 89 | 50.8%, 2.8 | 71.4%, 1.3 |
  | sample | 6.1%, 7.7 | 74.5%, 0.4 | 83.8%, 0.2 |
  | sample compact | 33.2%, 753 (42.2%, 74 one level finer) | 57.8%, 1.4 | 84.1%, 1.0 |

  The same picture on every board: fine parts recognise their situation almost always (over 95% of their bursts), but a burst there is one or two flips, cheaper to simulate than to look up. Only the coarsest setting (two to seven parts) pays, and it adds 5% to 25% on the user's CPU and a third on the compact sample. As measured, recursion below the first level would cost more than it saves; it would need a lookup about as cheap as one flip (a small part's state and inputs used directly as an index into a table, not hashed).

  **Small parts read from a table: measured, not worth building.** The estimator counts the parts small enough to index a table directly (their simulated signals and inputs, each with "changed in the last tick", in at most 16 or 24 bits). Nearly all fine parts are that small, and together they hold 30% to 80% of the flips left in the misses. But they flip less often than their inputs change: 0.4 to 0.9 flips per lookup on the user's CPU and the compact sample, about 0.2 on the sample. A table lookup, however cheap, is then more work than the simulation it replaces, which already leaves a gate asleep when an input change cannot alter its output (attempt 5). Fine parts are closed as an idea unless a way is found to not look at a part when its input changes without effect, which is what attempt 5 does per gate.

  **A tree whose depth follows the parts** (the user's idea, 2026-11-19): do not split a part that is small enough, split one that is still large, and use a part as a unit only where that pays. The estimator now keeps the numbers per part and picks, from the root down, for every part the best of three: use it as a unit, split it, or leave it to the plain simulation. With a lookup counted as 8 flips, over 3 million ticks, on top of whole-board replay:

  | Board, program | Units chosen | Share of the missed flips skipped | Flips per lookup | Coarsest level alone, for comparison |
  |---|---|---|---|---|
  | the user's CPU, day 4 part 2 | 1 (the large part) | 22.9% | 226 | 22.9% |
  | the user's CPU, day 1 part 2 | 5, at four different depths (45 to 272 entities) | 50.9% | 18.7 | 4.5% |
  | sample | 2 | 15.2% | 16.3 | 6.1% |
  | sample compact | 1 | 41.5% | 149 | 33.2% |

  With a lookup at 4 flips the user's CPU on day 4 part 2 gets 7 units and 41.6%. So variable depth helps where the coarsest part rarely recognises its situation: a few mid-sized parts with few inputs (1 to 20) that know their situation 83% to 100% of the time carry the gain, and the hundreds of tiny parts are never chosen. Two cautions: the choice was made here with the whole run known, where an engine would have to learn while running which parts pay; and the units are chosen by their own numbers, without the cost of the machinery around them. This is the form of part-level skipping worth building, if any.

  **Can the tree be fixed at start-up by part size alone?** (the user's question.) Tested: split a part while it has more than `largest` entities, use what comes out if it has at least `smallest`. Net flips saved per tick, a lookup counted as 8 flips:

  | Rule (largest, smallest) | the user's CPU, day 4 part 2 | day 1 part 2 | sample | sample compact |
  |---|---|---|---|---|
  | never split (4096, any) | 0.45 | 0.11 | -0.19 | 0.77 |
  | (1024, 32) | -0.16 | 0.22 | -0.45 | -0.84 |
  | (512, 32) | -0.12 | 0.32 | -0.36 | -0.96 |
  | (256, 32) | 0.16 | 0.44 | -0.16 | -0.68 |
  | (64, 32) | 0.15 | 0.18 | -0.25 | -0.05 |
  | chosen per part with the run known | 0.45 | 0.82 | 0.07 | 0.93 |

  No size setting is good on all four, and the best one differs per board and even per program on one board. Size is useful as a floor only: no part under about 32 entities ever paid. So the tree's shape can be built at start-up from the netlist, but which of its parts are used has to follow what each part does while running (count its lookups and the flips they save, switch it off when it loses).

See "Not planned" in the ranked plan. Nothing else yet.

## Stage results

### Stage 1 (closed 2026-11-19)
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

v2 is the default engine from this commit; `--engine v1` runs the reference and `--verify` runs both. Not done from the end-of-stage list: re-measuring each accepted attempt by switching it off (most build on each other and cannot be switched off alone). `--verify` compares the number of VMem events and a digest of them, the stop and the final VMem; it says that the events differ but not at which tick. What was learned and every open idea: `vcb-engine-v2.1.md`.
