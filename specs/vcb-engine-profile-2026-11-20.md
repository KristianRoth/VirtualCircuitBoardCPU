# Profile of the v2 engine, 2026-11-20

The engine at commit `2213f1f` (attempt 32). The last profile before this one was step 0 of v2.1, at attempt 15; attempts 16 to 32 came after it. Apple M5, macOS 26.6. Nothing else heavy was running.

## How it was taken

- **Sampling**: macOS's `sample`, once a millisecond, on the whole run of each program. `engine/tools/sample_summary.py` turns its output into the samples whose top of stack is in each function, per thread.
- **The build sampled**: `--cfg separate`, new with this profile: a plain release build in which the engine's main functions are kept out of each other (`step_real`, `propagate`, `propagate_split`, `advance`, `intern`, `restore`), with no counters. In the build that is used every day (profile-guided) the compiler puts nearly all of them inside the run loop, and `sample` cannot tell which instruction of a function a sample was in, so that build shows only one large function. Its figures are given beside for day 4 part 2. The separate build runs day 4 part 2 in 7.4 s, the profile-guided one in 7.0 s, so the shares are close to the real ones but not the same.
- **Counts**: `examples/split_time.rs` in a `--cfg profiling` build, for ticks, flips and steps of the same runs.

```
RUSTFLAGS="--cfg separate -C target-cpu=native" cargo build --release --target-dir target/separate
target/separate/release/vcb run <board> --asm <program> --ticks <n> & sample $! 9 1 -file out.txt
python3 tools/sample_summary.py out.txt
```

## Where the main thread's time goes

Share of the main thread's samples. Day 4 part 2, day 3 part 2 and day 1 part 2 run to their Breakpoints on the user's CPU; the sample for 1,500 million ticks; the compact sample (a Timer, interval 1,000) for 500 million.

| Where | day 4 part 2 | day 3 part 2 | day 1 part 2 | sample | compact sample |
|---|---|---|---|---|---|
| `propagate_split`: telling readers, in a step with a record | 43.3% | 44.9% | 43.5% | 61.6% | 16.9% |
| `step_real`: the queue, evaluating, applying flips, the rest of a tick | 17.8% | 19.3% | 18.6% | 9.4% | 19.1% |
| `leave_split`: ending a step with a record | 7.6% | 6.8% | 6.3% | 4.1% | 7.0% |
| `begin_step`: starting a step (finding its context and record) | 6.2% | 6.0% | 2.5% | 2.6% | 1.7% |
| `propagate`: telling readers, in a step simulated whole | 4.6% | 2.8% | 9.3% | 2.0% | 32.5% |
| `restore`: putting a replayed state into the circuit | 2.7% | 2.2% | 3.8% | 2.9% | 10.5% |
| `advance`: the replay lookups, idle jumps, the loop over ticks | 2.2% | 3.7% | 2.5% | 3.8% | 3.8% |
| `Table::intern`: hashing and finding or storing a state | 2.0% | 1.7% | 0.8% | 1.4% | 0.6% |
| `Engine::intern`: what flips next, for the state's key | 1.9% | 1.3% | 3.0% | 0.8% | 0.6% |
| copying memory (mostly new states into the table) | 2.7% | 2.3% | 3.8% | 3.2% | 1.7% |
| `vmem_tick`: VMem loads and writes | 1.7% | 1.9% | 3.0% | 0.9% | 1.3% |
| the allocator | 1.6% | 1.9% | – | 0.8% | 0.5% |
| `Table::arrival`: where a replayed step leads | 0.8% | 0.9% | – | 2.5% | 0.9% |
| the run loop in `main.rs` (stop checks) | 0.8% | 1.0% | – | 1.5% | 0.6% |
| growing the table's hash maps | 0.6% | – | – | 0.8% | – |
| comparing states | 0.6% | – | – | – | – |
| clearing memory | – | 0.6% | 0.8% | – | – |
| Worker thread: share of the run it was busy | 14% | 1% | 2% | 2% | 1% |

The same run of day 4 part 2 in the profile-guided build: the run loop with everything inlined into it 79.1%, `leave_split` 6.5%, `begin_step` 6.2%, `vmem_tick` 2.4%, copying memory 2.0%, the allocator 1.6%, growing hash maps 0.7%. So the two functions that stay separate there have the same shares as in the separate build.

## What the runs did

| | day 4 part 2 | day 3 part 2 | day 1 part 2 | sample | compact sample |
|---|---|---|---|---|---|
| Ticks | 1,339,421,264 | 419,669,712 | 70,339,472 | 1,500,000,000 | 500,000,000 |
| Replayed | 65% | 68% | 46% | 95% | 75% |
| Jumped as idle | 30.7% | 28.1% | 47.2% | 4.6% | 20.4% |
| Simulated one by one | 4.5% (60.0 million) | 3.9% (16.2 million) | 7.2% (5.1 million) | 0.4% (5.5 million) | 4.6% (22.9 million) |
| Steps with a record | 2,969,970 | 768,008 | 245,906 | 278,891 | 736,284 |
| Steps simulated whole | 155,443 | 47,800 | 10,812 | 5,107 | 5,214 |
| Proofs | 23,341 | 1,521 | 663 | 1,319 | 1,515 |
| Flips | 364.6 million | 94.6 million | 30.1 million | 35.4 million | 176.7 million |
| of them in steps with a record | 319.0 million (87%) | 85.1 million (90%) | 23.2 million (77%) | 33.0 million (93%) | 58.8 million (33%) |

At step 0, day 4 part 2 simulated 9.9% of its ticks one by one; now 4.5%.

## What a thing costs (day 4 part 2, separate build, 7.4 s)

| Thing | Time | Count | Each |
|---|---|---|---|
| A flip in a step with a record (`propagate_split`) | 3.20 s | 319.0 million | 10.0 ns |
| A flip in a step simulated whole (`propagate`) | 0.34 s | 45.6 million | 7.5 ns |
| A simulated tick apart from telling readers (`step_real`) | 1.32 s | 60.0 million | 22 ns |
| Ending a step with a record (`leave_split`) | 0.56 s | 2.97 million | 190 ns |
| Starting a step (`begin_step`) | 0.46 s | 3.13 million | 147 ns |
| Everything of replay and its table (`restore`, `advance`, both `intern`s, `arrival`, copying, growing, comparing) | 0.84 s | | |

A step with a record: 107 flips and 18 ticks, about 1.8 µs with its start and end. A whole step: 293 flips and 37 ticks, about 3.2 µs. (The whole steps' figures are less sure: timer ticks and the rest of interrupted steps are in them.)

## What it says

1. **The hottest function is now `propagate_split`, with 43 to 45% of the run on the user's CPU and 62% on the sample.** `propagate`, which attempts 25, 26 and 31 worked on and whose machine code is in `vcb-engine-propagate-asm.s`, is down to 3 to 9%: nine flips in ten are made in a step with a record, and those go through `propagate_split`. That function was written in attempt 27 and has never been looked at: its machine code has not been read, and nothing in it has been tuned. **A flip costs 10.0 ns in it against 7.5 ns in `propagate`**: it tests for every reader whether the reader is in the variable part, takes the record out of the engine and puts it back at every call, and hands over the record's fixed flips. If it cost what `propagate` costs per flip, the run would be about a tenth shorter.
2. **The rest of a tick (`step_real`) is 18 to 19%**, 22 ns per simulated tick. A step with a record has 18 ticks, so this is about 0.4 µs of its 1.8.
3. **Starting and ending a step with a record is 14%** (190 ns and 147 ns), after attempts 29 and 30 took what was easy there. Ending is the dearer of the two.
4. **Replay and its table are about 11%** together, in many small pieces; the largest are putting a state back (2.7%) and copying new states into the table (2.7%).
5. **The worker thread is busy 14% of the time on day 4 part 2** and 1 to 2% elsewhere, 70% of that in `proof_tick`. It is never behind (attempt 28), so this costs a second core's time and nothing on the clock.
6. **The compact sample is another picture**: a third of its time is `propagate` in ticks simulated whole (two flips in three there are outside a step with a record: the Timer's ticks and what follows them), and `restore` is 10.5%, four times its share elsewhere. Attempt 32 could not move the first; the second has not been looked at.

## What the profile does not show

- Which instructions inside a function are slow: `sample` gives no count per address, and Xcode's Instruments (which does, and counts mispredicted branches) is not installed here, only the command-line tools.
- The profile-guided build's own split of the run loop: there the functions are inlined. The separate build stands in for it.
- Cache misses in the replay table (up to 1.3 GB on day 4 part 2): the table's functions are a few percent together, which bounds what misses can cost, but they were not counted.
