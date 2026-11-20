# Research 3: one list of findings

Written 2026-11-20. It merges three reports by three agents (Opus 5.5), each one agent's reading, none checked by a second reader:
- **G**: `research3-rust-guides.md` (hotpath.rs, a users.rust-lang.org thread, the rust-trends 2026 guide),
- **P**: `research3-perf-book.md` (The Rust Performance Book, every chapter),
- **S**: `research3-simd.md` (is SIMD or vectorising any help).

The code they read is the engine at commit `01fee77` (attempt 28). **Nothing was built or measured in the engine.** S timed copies of some loops in a scratch crate and looked at the engine's binary; where a number comes from that, it says so. Everything else is an estimate. The line numbers are in the three reports.

## The answer on SIMD

- **The tick itself: no.** It is about 88% of the run and touches 6 to 8 scattered gates and short reader lists. This machine (Apple M5) has no gather or scatter, and two updates in one tick can hit the same count.
- **Many ticks or many data variants in one word: no.** The published bit-parallel simulators need circuits that are levelised, without unit delay or without feedback inside a cycle. This one has unit delay and feedback everywhere, so it would do at least the work of the flips.
- **The proof on the worker: no.** Only its delay matters, and the worker is never behind.
- **The packed-state loops: a little.** Together about 2 to 5% of a step with a record, and the largest item there is not SIMD (finding 1).
- **What stable Rust has here:** NEON at 128 bits, by hand or by the compiler. Portable SIMD, SVE and SME are all unstable, and the M5 has SVE only in a special mode.

## Findings, most promising first

Each is small. The three reports agree on that: together perhaps 3 to 8% of a run, and that sum is an estimate.

| # | Finding | From | Size | Basis |
|---|---|---|---|---|
| 1 | **The replay table's hash keeps its four lanes on the stack**, so they wait for each other. With the lanes in registers it is 3.3 times faster and gives the same values. | S | 34 to 10 ns for 50 words, once per simulated step: about 1.5% of a step with a record | Seen in the engine's binary; timed in a copy |
| 2 | **The check whether a record fits is scalar**, with an early exit and six bounds checks per word. As an OR over all words with one test at the end the compiler vectorises it. | S, G, P | 42 to 14 ns per record checked to the end: 2 to 3% of a step with a record | Seen in the binary; timed in a copy |
| 3 | **The context is looked up twice per step with the slow default hasher**: at the step's start and again at its end, same key. Remember the position; hash the key cheaply. | G, P, S | 20 to 40 ns per step | Read in the code; cost assumed |
| 4 | **Two functions that became hot are still marked for the rare path.** `leave_split` is marked cold but runs once per step with a record; `mode_tick` is forced out of line but runs after every tick of such a step, and updates two counters always. | G, P | A few ns per tick of such a step | Read in the code; cost assumed |
| 5 | **The queue push inside the innermost loop can grow memory**, so the compiler may reload the array pointers after every push. The queue can never hold more than there are entities, so the growth path can go. Look at the machine code first. | G, P | Unknown; the only finding aimed at the function that takes most of the time. Attempt 14 gained 14 to 30% from a similar effect | Assumed; needs the assembly |
| 6 | **A replayed step makes 3 to 4 dependent accesses into a table of up to 1 GB.** Keeping the last word seen and where it led inside the edge would skip one lookup for instruction fetches, which nearly always load the same word. | P | 3 to 6% of a run if those accesses miss the cache | Assumed from the table's size; a counter first |
| 7 | **The step's start evaluates again what the step before just evaluated.** `intern` works out which queued components will flip; `begin_step` then works out the same set. | P | Small, but it removes work, not just instructions | Read in the code |
| 8 | **The finished-records channel is polled through a lock at every step start**, about 730,000 times per 300 million ticks, nearly always for nothing. One atomic counter read would do. | G, P | 10 to 20 ns per step; under 1% | Read in the code; cost assumed. P says only if a profile shows it |
| 9 | **Two 64-bit divisions per `meta()` call**, twice per step, by periods known only at run time. Keep the positions as counters, as attempt 13 did for the due ticks. | G, P | A fraction of a percent | Read in the code |
| 10 | **A record's net change as an XOR mask over the packed words.** Saves only half of each entry's work, because every signal's own bit must still be set one by one. | S | 15 to 30 ns per step: 1 to 2% of a step with a record | Timed in a copy; saving inferred |
| 11 | **`flip_bit` tests whether replay is on at every trace change.** Only Random boards have no replay now, so the test could go. | P | Small; the branch is always predicted | Read in the code |
| 12 | **Bounds checks remain in the per-step code** (`leave_split`, `count_fixed`, `restore`, `intern`). | G, P | A few ns each, once per step | Read in the code |

Findings 1, 2, 3, 4, 7, 8, 9 and 12 are all in the code that starts and finishes a step, which is about a third of a step with a record. They would make one attempt, or two.

## Measurements the reports ask for before building

| Measurement | Decides | From |
|---|---|---|
| The machine code of `propagate` and `feed`: are array pointers reloaded after the push? | Finding 5 | G, P |
| A profile of the real profile-guided build with line tables (`debug = "line-tables-only"` does not change the code), and Instruments' counters for branch mispredictions per flip | Where inside the tick the time goes; whether a branch-free `feed` is worth trying | P, S |
| How often a replayed load brings the same word as last time for the same edge | Finding 6 | P |
| How old a record is when it arrives, whether a proof is ever dropped because the queue is full, and how many steps start while their context's proof is still running | Whether a faster proof would matter at all | G, S |
| Whether the step start's 0.17 µs is steady or a mean over a long tail | Where in finding 3 and 8 the time is | G |
| Allocations on the main thread in a 20-million-tick run | Whether another allocator could matter (expected: no) | G |

hotpath (the profiler) could give the middle three; G also describes how to get them with a few counters and no new dependency.

## Checked and closed

- **SIMD in the tick, table lookups for gate evaluation, bit-parallel simulation across patterns, time or data variants, bit-planes in the proof** (S): see the answer above.
- **Another allocator** (G, P): the tick and the common step path allocate nothing; a few thousand allocations per program remain.
- **Prefetching** (G): everything hot is in the first cache.
- **Branch-free tricks in `feed`** (G, S): not unless the misprediction measurement says so; attempts 25 and 26 showed that changes in that loop cost more than they save.
- **A faster or bit-packed proof** (G, S): it runs on the worker, which is never behind.
- **An AES-based hash** (S): 6 ns better than finding 1 and its values differ; not worth a new hash function.
- **Smaller record entries, space reserved for the replay table up front** (P): nothing in time expected; a one-line size check on the 8-byte cell is worth adding as a guard.
- **crossbeam or another channel, more threads, rayon** (G, P): nothing to gain.
- **Everything in the build settings** (P): already done; `target-cpu=native` resolves to `apple-m5`.

## Outside the benchmark

- **`--verbose` writes one line per watched memory write through an unbuffered, locked stdout** (P). It probably sets the speed of a verbose run. The benchmark does not print.
- **Clippy's performance lints have never been run on the crate** (P). One command.

## How far to trust this

- Findings 1 and 2 rest on the engine's binary and on timings of copies. S notes the binary it read may have been a plain build, not the profile-guided one.
- Every other size is an estimate from reading the code.
- All three agents read their web sources through a summarising tool, so quoted wording may be paraphrased.
- None of the three reports was checked by a second reader, and I have not verified their line numbers.
