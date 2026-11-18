# VCB engine v2: the optimisation log

Every optimisation that was tried on the v2 engine, what it gained, and whether it was kept. The process, the acceptance rule and what "equal to v1" means are in `vcb-engine-v2.md`.

## Summary

| # | Attempt | Effect on the mean | Verdict |
|---|---|---|---|
| 0 | Scaffolding: v2 as a copy of v1's core | not measured separately (the same code) | kept |
| 1 | No allocation in a tick | 1.68 (quick run) | accepted |

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

Not done yet from the spec's Scaffolding: the event primitive ("advance to the next event or to tick N") and `--verify`. The profile showed the per-tick run loop costs nothing measurable, so attempts 1 to 6 do not need the primitive; it is due before idle-tick jumps or replay. v2 against v1 was not measured as its own row: it is the same code, and the "old" column below (5.9 to 6.4) is v1's speed.

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

## Ideas not tried

See "Not planned" in the ranked plan. Nothing else yet.

## Stage results

None yet.
