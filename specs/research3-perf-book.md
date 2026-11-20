# Research 3: The Rust Performance Book applied to v2

Written on 2026-11-20 by one agent, which was asked for ideas, not proven wins. The source is *The Rust Performance Book* (https://nnethercote.github.io/perf-book/), all chapters: Benchmarking, Build Configuration, Linting, Profiling, Inlining, Hashing, Heap Allocations, Type Sizes, Standard Library Types, Iterators, Bounds Checks, I/O, Logging and Debugging, Wrapper Types, Machine Code, Parallelism, General Tips and Compile Times. The code is the engine at commit `01fee77` (attempt 28).

Tags used below:
- **[code]**: checked by reading the code at the line numbers given.
- **[assumed]**: an inference or estimate that has not been measured. Each one needs the test listed with it.

None of these ideas has been measured. The known facts that bound them: everything a simulated tick touches fits the L1 cache; a tick costs about 70 ns and a flip 10 to 13 ns; a step with a record costs about 1.6 µs, a third of which is starting and finishing it; telling readers costs almost nothing (attempts 25 and 26); renumbering did nothing (attempt 21).

## 1. The most promising

1. **R1. Keep the arrival for the last word seen inside the edge** (General Tips, Type Sizes). Each replayed step follows a chain of 3 to 4 dependent memory accesses into a table of up to 1 GB. This is the one place where memory, not the L1 cache, may set the cost.
2. **R2. A `dirty` push without the capacity check** (Heap Allocations, Bounds Checks, Machine Code). This is the hottest loop's only remaining call that can grow memory. Look at the machine code first.
3. **R3. Replace SipHash on `split.contexts` and drop the second lookup per step** (Hashing). This is a one-line change to test.
4. **R4. Bring the per-tick part of `mode_tick` back inline** (Inlining, Logging and Debugging). Today every tick of a split step makes a non-inlined call.
5. **R5. Clean up the code that runs once per step**: `begin_step`, `leave_split`, `intern` and `restore` (Bounds Checks, Iterators, General Tips). This is where the third of a split step goes that is "starting and finishing it".
6. **R6. Profile the real PGO build at line level** (Profiling). Today's `--cfg profiling` build changes inlining, so it does not show the code that actually runs.

## 2. Every idea

### R1. The arrival for the last word seen, kept inside the edge
- **Book.** General Tips: "Use compact representations for common values, with a fallback secondary table for unusual ones" and "Put a small cache in front of a data structure for lookups with high locality". Type Sizes: smaller hot types mean less cache pressure.
- **Where** [code].
  - `sim2.rs` `advance`, lines 951 to 975, calls `table.edge(self.node)` and then `table.arrival(...)`.
  - `replay.rs` `edge` (lines 195 to 198) reads `nodes[from].edge` and then `edges[at]`.
  - `arrival` (lines 202 to 208) does one more `by_word` hash lookup (a hashbrown control group, then a bucket) for every edge that ends in a load.
  - The next replayed step starts at `nodes[arrival.node]`.
  - `Node` is 48 bytes (lines 27 to 36). `(Edge, Arrival)` is 32 bytes (line 81).
- **Why it might help.**
  - [assumed] The table reaches 994 MiB on day 4 part 2 (attempt 22), so these accesses are probably cache or TLB misses, and they depend on each other.
  - Most events on a CPU are loads. For a given node the load address is fixed, and for an instruction fetch the word is almost always the same. So keeping `(last_word, last_arrival)` in the edge's own entry would skip the `by_word` probe most of the time.
  - Putting the edge into `Node` would also remove the `nodes` to `edges` step, at the cost of a larger `Node`.
  - `Node` has room to give:
    - `rose_len` is always 0, because `intern` is always called with `&[]` (`sim2.rs` lines 868 and 873) [code].
    - `hash` is only needed to rebuild `by_hash` on load, and could be recomputed there.
  - Rough size [assumed]: about 5 million replayed steps on day 4 part 2. At about 190 ns each (from the 7% replay share in the step 0 profile) they take about 1 s. Removing one miss in two or three could be worth 3 to 6% of a run.
- **Why it might not.**
  - Data loads vary, so they still go to `by_word`.
  - Hardware prefetching and the 16 KB pages of macOS may already hide more of this than I assume.
  - Replay was 7% at step 0. If it is still about that, there is little to gain.
- **Cheapest test.**
  - First, a profiling counter in `arrival`: the share of calls whose word equals the previous word for the same edge.
  - Then the inline cache alone. That means two fields more in the `edges` tuple and a compare before `by_word.get`, about 15 lines in `replay.rs`, with the save format left alone except that the cache stays empty after a load.
  - Run the quick benchmark, and also day 4 part 2 run to its end, since replay grows over a run.

### R2. A `dirty` push without the capacity check
- **Book.** Heap Allocations, "Longer Vecs": `with_capacity` and `reserve`. Bounds Checks: unchecked access is the last resort. Machine Code: inspect the generated code of small, very hot code.
- **Where** [code].
  - `sim2.rs` `feed`, line 580, runs `self.dirty.push(reader)` in the innermost loop of `propagate` (lines 517 and 533).
  - The same applies to `flips.push(e)` in `step_real` (line 668) and to `self.split.traces.push` in `trace_split` (`split.rs` line 576).
  - `dirty` can never hold more entries than there are entities, because every push is guarded by the `queued` flag. I checked every push site: `feed` line 578, `new` line 425, `leave_split` line 668.
- **Why it might help** [assumed]. `Vec::push` keeps a capacity compare and a call to the growth routine in the loop. A possible call in the middle of a loop over `self.readers` and `self.cell` can force the compiler to reload `self`'s fields after it, the same effect that attempt 14 found with bounds checks (1.14 overall, 1.30 on a whole run). If `dirty` were given capacity `n` in `new` and filled with an unchecked write plus `set_len`, or kept as a fixed `Box<[u32]>` of length `n` with its own `u32` length, the call would leave the loop.
- **Why it might not.** LLVM may already move the reloads onto the cold path (partial redundancy elimination), and PGO already lays the grow call out of line. Then nothing changes.
- **Cheapest test.**
  - Look at the machine code of `step_real` in the plain release build (`cargo install cargo-show-asm`, which is not installed here; then `cargo asm vcb::sim2::Engine::step_real`). Check whether field loads such as `readers.items.ptr` and `cell.ptr` are repeated after the `bl` to `grow_one` or `reserve_for_push`.
  - If they are, make the change: about 10 lines, with a `debug_assert!(len < capacity)` kept in. Then the quick benchmark.

### R3. A non-SipHash hasher for `split.contexts`, and one lookup per step, not two
- **Book.** Hashing: the default hasher is SipHash 1-3, which is slow for short keys. FxHash showed 4 to 84% in rustc's measurements against the default. Byte-wise hashing helps when `#[derive(Hash)]` hashes field by field. Linting: `disallowed_types` can ban `std::collections::HashMap` so the default is not used by accident.
- **Where** [code].
  - `split.rs` line 164, `contexts: HashMap<Meta, Context>`, uses the default hasher.
  - `Meta` (`replay.rs` lines 15 to 24) has a derived `Hash` over five fields.
  - It is looked up in `begin_step` at line 275 (`entry(key).or_default()`) and again in `leave_split` at line 676 (`get_mut`), so twice for every step with a record. `collect` and `install` (lines 400 and 413) look it up again when records arrive.
- **Why it might help** [assumed]. Each lookup hashes about 14 bytes through SipHash in five `write` calls, roughly 20 to 30 ns. Two per step against 1.6 µs is about 3% of a split step, or 1 to 2% of a run.
  - The second lookup exists only because `begin_step` moves the record out (`records.remove(found)`, line 280) and `leave_split` puts it back. If the record stayed in the context and only an index or a pointer were held, the second lookup would go.
  - `replay::Mix` is already public and could be the hasher. Better still, a hand-written `Hash for Meta` that packs the fields into two `write_u64` calls.
- **Why it might not.** The contexts are a few hundred to a few thousand, so the map sits in L2 and the hash is all there is to save, and 1 to 2% is near the bar of 1.015.
- **Cheapest test.** Change the type to `HashMap<Meta, Context, BuildHasherDefault<Mix>>`, one line, and run the quick benchmark. If it pays, remove the second lookup next.

### R4. The per-tick part of `mode_tick` inline; counters only when profiling
- **Book.** Inlining: "Entry and exit from hot, uninlined functions can take significant time". When only one call site is hot, split the function into an `#[inline(always)]` hot part and an `#[inline(never)]` cold part. Logging and Debugging: no needless work when statistics are not wanted.
- **Where** [code].
  - `split.rs` `mode_tick` (lines 469 to 486) is `#[inline(never)]` and is called after every tick while `split.mode != NORMAL` (`sim2.rs` lines 723 to 725).
  - On the main thread with a worker (the default, `threads = 1`) the mode is never `PROOF`, so this call is the per-tick bookkeeping of split steps:
    - two counters, updated always and not under `cfg(profiling)` (lines 473 and 474);
    - an `unwrap` of the record;
    - a `timed.get(cursor)`;
    - a store of `due`.
  - `propagate_split` (lines 490 to 551) also takes the record out of its `Option` and puts it back on every call (lines 491 and 550).
- **Why it might help** [assumed]. Split steps are 93% of the steps replay misses (attempt 27), and about 18 ticks each. One call and return per tick, plus the register saving around it, may be a few ns of a 70 ns tick.
- **Why it might not.** The call is predictable, and PGO may already lay it out well. The gain is a few percent at most.
- **Cheapest test.** Move the SPLIT branch (the part before `leave_split`) into an `#[inline(always)]` function called from `step_real`. Keep `leave_split` and `proof_tick` cold. Put the two counters under `cfg(profiling)`, or count them per step in `leave_split`. Then the quick benchmark.

### R5. The code that runs once per step: no bounds checks, a branchless check, no repeated evaluation
- **Book.** Bounds Checks: iterate instead of indexing, slice before the loop, use `get_unchecked` as a last resort. Iterators: use `zip` or `copied` so LLVM can produce better code. General Tips: "avoid computing things unless necessary", and caching. Machine Code: `core::arch` and SIMD.
- **Where and what** [code].
  - `begin_step`, line 276: the closure `fits` indexes five slices by `w` inside `(0..words).all(...)`, for up to six records. That means a bounds check on each access and an early exit per word, which stops it from being vectorized. A version that iterates with `zip` and ORs all words before one test could run on NEON. A record start has about 33 words (2,100 signals).
  - The learning loop in `begin_step` (lines 301 to 307) has the same pattern.
  - `leave_split` (lines 611 to 679) is cold, but it runs once per split step with about 30 to 50 `net` and 60 to 80 `counts` entries (from the group A table) and every reader of the last tick's sources. It indexes everything with checks (`self.cell[...]`, `lists.start[...]`, `lists.items[i]`). The same holds for `count_fixed` (lines 583 to 605), `restore` (lines 882 to 929) and `intern` (lines 857 to 864).
  - **Repeated evaluation** [code]:
    - At the end of a simulated step, `intern` (lines 859 to 864) evaluates every queued component to build `pending`.
    - Right after that, the next `advance` calls `begin_step`, whose `first_flips` (lines 251 to 257) evaluates the same queued components with the same rule and builds the same set as bits.
    - When the previous step ended in `intern`, `first_flips` could build its bits from `self.pending` instead.
  - `meta()` (`sim2.rs` line 844) computes `tick % clock_interval` and `schedule.key(tick)`, which is two 64-bit divisions. It is called in both `intern` and `begin_step`, so four divisions per step that could be two (or none, with the phase kept as a counter, as attempt 13 did for the tick).
- **Why it might help** [assumed]. The log says a third of a split step's 1.6 µs is starting and finishing it, about 0.5 µs. Each item here is small. Together they might take 10 to 20% off that third, which is 2 to 4% of a run.
- **Why it might not.** Bounds checks that are predicted correctly cost little outside tight loops. Attempt 14's large gain came from register pressure inside the tick's loops, which these functions mostly do not have.
- **Cheapest test.** One attempt that does all of it in these four functions: `g` and `gm`, `fits` as an OR over zipped words, `first_flips` taking `pending` when the arrays are those of the event just interned. Then the quick benchmark. If it is under the bar, keep only the `pending` reuse, which removes evaluations and is not just cheaper code.

### R6. Profile the build that actually runs, line by line
- **Book.** Profiling: `debug = "line-tables-only"` in `[profile.release]` for profilers, `-C force-frame-pointers=yes`, and Instruments or samply on macOS. Use more than one profiler. Benchmarking: instruction or cycle counts vary less than wall time.
- **Where.**
  - [code] The profiles so far came from a `--cfg profiling` build, which marks `propagate`, `step_real`, `intern` and `restore` as `#[inline(never)]` and switches on counters. That build has a different code layout from the PGO build that runs.
  - `Cargo.toml` has no debug info in release.
- **Why it might help.** It finds where inside `step_real` (where `propagate` is inlined) the cycles go, and whether R2 or R4 are real, before any code is written. Instruments' CPU Counters can show branch mispredictions and instructions per flip, which is also what the second research pass's C5 asked for.
- **Why it might not.** Line attribution in a PGO, LTO-fat, single-codegen-unit binary is approximate.
- **Cheapest test.** Run `CARGO_PROFILE_RELEASE_DEBUG=line-tables-only sh tools/build-pgo.sh` (debug info does not change the generated code), then `xctrace record --template 'Time Profiler'` on day 4 part 2. `xctrace` is installed; samply is not.

### R7. `flip_bit` without its branch
- **Book.** Machine Code, and General Tips: handle the common case without a test.
- **Where** [code]. `sim2.rs` `flip_bit` (lines 541 to 546) tests `self.replay.is_some()` on every trace that changes (`propagate` line 528, `trace_split` line 575). `step_real` already moves this test out of its loop (`packing`, line 693), but `propagate` does not.
- **Why it might help.** Since attempt 23 only boards with Random ink have no replay. Keeping `packed` up to date on those too would make `flip_bit` an unconditional load, XOR and store, and remove a branch and a reload of `self.replay` per trace flip. Entities that are not simulated already point at the spare word, and `packed` already exists on every board (`new`, line 435) [code].
- **Why it might not.** The branch is always predicted the same way. Attempt 19 measured a 5% loss when a board without replay kept the copy, but that board was the compact sample, which has replay now.
- **Cheapest test.** Delete the `if` in `flip_bit` and the `packing` test in `step_real`. Run the quick benchmark, and one Random board for its own speed.

### R8. Smaller entries in the record's lists
- **Book.** Type Sizes: smaller integers, compact representations, `static_assertions` to keep a size from growing again.
- **Where** [code].
  - `Record.flips` and `Record.timed` (`split.rs` lines 68 and 70) are `Vec<(u32, u32, bool)>`, 12 bytes each.
  - `timed` is walked on every tick of a split step (`propagate_split` lines 499 and 535). `flips` is walked with `partition_point` and in a loop in `leave_split` (lines 616 and 627).
  - Packing the value into the entity's top bit gives 8 bytes per entry.
  - Separately: `Cell` must stay 8 bytes. A `const _: () = assert!(std::mem::size_of::<Cell>() == 8);` guards it without any crate.
- **Why it might not.** Everything is in L1 already (attempts 20 and 21), so this saves almost nothing in time. The size assert is a guard, not a speed-up.
- **Cheapest test.** Add the size assert now. Try the packing only alongside R5.

### R9. Space for the replay table reserved up front
- **Book.** Heap Allocations: `Vec::with_capacity` and `reserve` for long vectors; growth doubles.
- **Where** [code]. `replay.rs` `intern` grows `bits`, `lists` and `nodes` with `extend_from_slice` and `push` (lines 160 to 163), and `by_hash` with `insert` (line 164). The table reaches about 1 GB, so it doubles and rehashes many times, each time copying everything. `clear` keeps the capacity (lines 101 to 110), so only the first fill pays.
- **Why it might help** [assumed]. The step 0 profile had "storing new states, copying, memory" at 3%. Some of that may be the doublings and rehashes.
- **Why it might not.** On macOS a large `realloc` may move pages without copying them. And reserving 2 GiB of address space costs page faults when the space is touched, not when it is reserved.
- **Cheapest test.** In `Table::new`, reserve `bits` and `nodes` for, say, a quarter of `memory_limit`. Run day 4 part 2 to its end.

### R10. A different allocator
- **Book.** Build Configuration: jemalloc (`tikv-jemallocator`) or `mimalloc` through `#[global_allocator]`.
- **Where.** The whole binary.
- **Why it might not** [code].
  - The tick allocates nothing (attempt 1).
  - The table grows in a few very large blocks.
  - The only frequent allocations are per proof (`Job` vectors in `begin_step` line 314, `Box<Record>` and its vectors in `make_record`), a few thousand per run.
  - The program has no allocation-heavy path, so a gain is unlikely.
- **Cheapest test.** Five minutes: add the `mimalloc` dependency and the two lines, then the quick benchmark. It adds a dependency, which the user should approve.

### R11. `--verbose` and `--probe` write to stdout once per line
- **Book.** I/O: `println!` locks stdout on every call, and stdout writes a full line at a time. Lock it once and wrap it in a `BufWriter`.
- **Where** [code]. `main.rs` `Run::simulate`, lines 631 to 641: one `println!` per watched VMem write (`--verbose`) and one per tick (`--probe`, which runs on v1 anyway).
- **Why it might help.** With `--verbose` at more than 100 million ticks per second and a write every few hundred ticks, that is hundreds of thousands of `write` system calls per second. It probably sets the speed of a verbose run. It does nothing for the benchmark, which does not print.
- **Cheapest test.** Time one verbose run of day 1 part 1 with output to `/dev/null`, against the same run without `--verbose`.

### R12. The finished-records channel without a `Mutex` and `Arc`
- **Book.** Wrapper Types: `Mutex` and `Arc` cost on every access. Parallelism: `crossbeam`.
- **Where** [code].
  - `split.rs` `Workers` (lines 139 to 142) holds the finished-records receiver in a `Mutex`.
  - `collect` (lines 396 to 409) runs at every step start. It takes the `Arc` out of an `Option`, `try_lock`s the mutex, `try_recv`s and puts everything back.
  - Only the main engine ever holds `workers`: `ask` clones the engine with `split` taken out (line 369), so the workers' copies never get it.
- **Why it might not.** An uncontended lock is one atomic operation and a few ns, against 1.6 µs per step. Expect under 1%.
- **Cheapest test.** Only if the profile (R6) shows `collect`. Otherwise do not try it.

### R13. Run Clippy's performance lints once
- **Book.** Linting: `cargo clippy`, the Perf group, and `disallowed_types`.
- **Where.** The whole crate. Clippy is installed and was not run for this report: running it writes into `target/`, which this report was not to change.
- **Why it might help.** It is cheap, and it might find things like R3 that reading missed.
- **Cheapest test.** `cargo clippy --release -- -W clippy::perf`.

## 3. Book advice checked and found done or not applicable

- Release build, `opt-level` 3: the default for `--release`, which is used [code].
- `codegen-units = 1`, `lto = "fat"`, `panic = "abort"`: in `Cargo.toml` [code].
- `-C target-cpu=native`: in `.cargo/config.toml`. On this machine `native` is `apple-m5`, with `neon`, `lse`, `dotprod` and others (checked with `rustc --print target-cpus`, rustc 1.100.0).
- PGO: `tools/build-pgo.sh`. Its training run is the default engine, so the split and the worker thread are trained too [code].
- BOLT (through `cargo-pgo`): not tried and not installed. I assume BOLT does not work on Mach-O binaries [assumed].
- Transparent huge pages through `MALLOC_CONF`: the book gives this for Linux only. Not applicable on macOS.
- A faster linker, `debug = false` in dev, Cranelift, the parallel front end, `cargo build --timings`, `-Zmacro-stats`, `cargo llvm-lines`, generic-code tricks: these are about compile time, not run time. The release build takes 9 s and the PGO build 70 s; nothing to gain for speed.
- Binary-size settings (`opt-level = "z"`, `strip`): not wanted here.
- A faster hasher for the replay table: `replay::Mix` on `by_hash` and `by_word` [code]. The book's `nohash_hasher` idea fits here (the keys are already hashes), and Mix's `write_u64` is one multiply and rotate, so it is already nearly that.
- Byte-wise hashing of the state: `Table::hash` mixes whole `u64` words in four lanes [code].
- No allocation in the tick and reused work lists ("workhorse collections"): `dirty`, `work`, `flips`, `pending`, `split.lists` and `split.sources` are reused [code].
- `SmallVec` or `ArrayVec` for short lists: the wiring is already flat (`Lists`, attempt 3) and nothing in the tick allocates.
- `Box<[T]>` or `ThinVec` for smaller containers: they save a word per `Vec` header, and there are no large arrays of `Vec`s in the hot path.
- Type sizes of the per-entity record: `Cell` is 8 bytes (attempt 20) [code].
- `Vec::remove` against `swap_remove`: `records.remove` and `insert(0, …)` work on at most 6 elements (`RECORDS`) [code].
- `ok_or` against `ok_or_else` and similar: only in start-up and error paths.
- `chain` in hot iterators: the `chain` calls in `split.rs` are in `proof_tick` and `make_record`, which now run on the worker, and in `leave_split`'s sources (cold). The tick's loops use index ranges [code].
- `chunks_exact`: no chunked loops in hot code.
- `collect` in hot code: none in the tick. The only `collect`s per step are in proof setup and `make_record` (worker, rare).
- Bounds checks in the tick: removed with `g`, `gm` and `Lists::of` (attempt 14) [code].
- `debug_assert!` instead of `assert!` in hot code: done. The only `assert!` is in `Table::record`, once per stored step.
- Inlining hints for the hot helpers: `g`, `gm`, `Lists::of`, `feed`, `flip_bit` and `evaluate` are `#[inline(always)]`; `vmem_tick`, `draw` and `leave_split` are `#[cold]` and `#[inline(never)]` (attempt 16) [code].
- `cfg(profiling)` counters that cost nothing when off: done through `cfg!` [code]. The exceptions are `split.counts` (R4).
- Stop checks and the progress line in the run loop: the clock is read every 65,536 calls (`main.rs` line 626), and the run loop was measured as nearly free (attempt 9) [code].
- `parking_lot`: no contended locks.
- rayon and splitting the work over threads: a tick is too short to split (v2.1, section D), and more than one proof worker gave nothing (attempt 28).
- Replay table I/O: `BufReader` and `BufWriter` on the cache file, and reading raw bytes with `read_exact` (`replay.rs` lines 231 to 264) [code].
- Benchmarking practice: `tools/bench.py` alternates two builds over ten programs with three boards; the quick run comes first.

## 4. What I could not read or check

- **How I read the book.** I fetched it as the single-page print version (`print.html`) through a tool that answers questions about the page, not as raw text. The tool's summaries covered every chapter in the table of contents, including the end of Compile Times. But I did not see every sentence, so a detail the summaries left out may be missing here. I did not open the pages the book links to, such as the Bounds Check Cookbook or the SIMD post.
- **Nothing was built, run or profiled.** All "[assumed]" estimates are arithmetic on the numbers in the log and the v2.1 file. In particular:
  - That replay lookups miss the cache (R1) is an inference from the table's size, not a measurement.
  - Whether `push` forces reloads in `propagate` (R2) needs the machine code, which I did not produce: `cargo-show-asm` is not installed, and building would have written into `target/`.
- **Not read:** `sim.rs` (v1) beyond `run_events` and the random generator, `netlist.rs`, and how boards are loaded. Start-up (about 0.07 s, a fifth of the shortest runs) was not looked at.
- **Not checked against the second research pass in detail.** I read only its headings. R6's branch-misprediction measurement overlaps its C5, and R1 is close to nothing there; group D is about hashing and replacement, not access paths.
