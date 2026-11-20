# Research 3: three Rust performance sources against the v2 engine

Written 2026-11-20 by one agent (Opus 5.5) at the user's request. Ideas, not winners. Nothing here was measured on the engine.

**Sources read**
- **H**: https://hotpath.rs/, plus the pages it links that hold the technique: `/profiling_overhead`, `/functions`, `/threads`, `/locks`, `/data_flow`, `/cpu_profiling`, `/blog/sampling_comparison` and `/blog/profiling-rust-guide`.
- **F**: https://users.rust-lang.org/t/high-performance-operations/19739. Seven posts (the opening post and six replies, August 2018), all read. The links in the replies go to playground and Compiler Explorer snippets and to a pre-RFC on `unchecked_*` arithmetic. Their content is summarised in the posts themselves, so they were not opened.
- **T**: https://rust-trends.com/posts/rust-performance-guide-2026/. Read twice, the second time for every sentence on a list of keywords. Its two links are newsletter issues about the case studies, which the article already describes, so they were not opened.

**Markers**
- **[code]**: checked by reading the repository at commit `01fee77`.
- **[assumed]**: my own knowledge or estimate, not checked here.
- Line numbers are for that commit.

**What the sources cover.** They are thinner than the questions asked of them:
- T has two case studies (an integer accumulator plus reciprocal multiplication; a 64-byte precomputed struct with no lock in the hot loop) and a five-step practical guide (profile first; layout and allocation before SIMD; check auto-vectorisation).
- F is about integer tricks: shift against division, mask against modulo, signed against unsigned. Its replies add a warning: most apparent differences come from versions that do different things, and LLVM already handles the small cases.
- H is a profiler, not a guide.

None of the three says anything about allocators as a speed-up, hashers, `likely`/`unlikely`, prefetching, bounds checks, or `#[inline]` as a speed tool. Where I answer those questions anyway, section 2b marks the answer as not backed by a source.

## 1. The most promising, in order

1. **Read the assembly of `propagate` and `feed`, and give the loop its arrays as separate borrows if it reloads them** (idea 1). F's replies say to compare the assembly before and after a change. Attempt 14 showed that this loop's cost is spread through its code. This is the only idea here aimed at the 60% of the time in `propagate`.
2. **Trim the start and end of a step with a record** (idea 2, a bundle):
   - one hash lookup per step instead of two, and a cheap hash for `Meta`;
   - no `Mutex` or channel poll when nothing is ready;
   - a branch-free, vectorisable `fits`.

   This aims at the third of a 1.6 µs step that is overhead. My estimate is 1 to 2% of a run: under the 1.015 bar alone, perhaps over it together.
3. **Use hotpath's channel, thread and per-call views once, as a measurement and not a speed-up** (idea 3). They answer three things sampling cannot: how old a record is when it arrives, how often `try_send` drops a proof, and the spread (P95) of `begin_step` and `leave_split` per call.
4. **The `SPLIT` half of `mode_tick` inlined; `leave_split` without `#[cold]`** (2b-iv). Not from a source, but it is the cheapest test on this list, and both functions now run on the common path.

The allocator (2b-i) and bitsets in the proof (2b-v) are probably not worth an attempt. The reasons are below.

## 2a. Ideas tied to a source

### Idea 1. The tick's loop in assembly; split borrows if it reloads through `self`
- **Sources.**
  - F: leonardo shows the assembly of `/ 2` against `>> 1`. dodomorandi advises checking optimisations and "comparing assembly". scottmcm shows that differences often come from the versions doing different things.
  - T, step 3 of its guide: "Check whether LLVM already auto-vectorized your loop before hand-writing intrinsics". T also says the optimiser doing "instruction selection, vectorization, and inlining" is LLVM for both languages.
- **Where.** `engine/src/sim2.rs`:
  - `propagate`, lines 498–537;
  - `feed`, lines 550–582, inlined into it;
  - `flip_bit`, lines 541–546, which tests `self.replay.is_some()` at every trace flip [code];
  - `step_real`, lines 640–726, which hoists that same test as `packing` at line 693 but only for its own loop [code].
- **Why it might help.**
  - `feed` takes `&mut self` and calls `self.dirty.push`, which can call the out-of-line `grow`. A pointer derived from `self` then escapes into a call [code: the code shape]. LLVM may therefore reload `self.cell`'s pointer, `self.readers` and the `replay` discriminant after every push, and keep less in registers [assumed: only the assembly can show it].
  - Attempt 14 removed bounds checks, which are a few instructions per index, and gained 14 to 30%. Instructions per reader told evidently count in this loop [code: the log].
  - If the reloads are there, `propagate` as a function over disjoint borrows would let LLVM keep the base pointers in registers. That means `cell: &mut [Cell]`, `lists: &Lists`, `packed: &mut [u64]`, `slot: &[u32]`, `dirty: &mut Vec<u32>` and `packing: bool`, borrowed once at its start.
- **Why it might not.**
  - Attempts 25 and 26 found that telling a reader is "nearly free", and that anything added to that loop cost more than it saved. Loads that hit L1 are cheap on an M5 [assumed].
  - PGO may already lay out the code well.
  - If the assembly shows no reloads, there is nothing to gain.
- **Cheapest test.** No code change:
  1. `cargo rustc --release -- --emit asm`, or `objdump -d --no-show-raw-insn target/release/vcb`, which is in CommandLineTools.
  2. Use a build with `--cfg profiling`, which keeps `propagate` as its own function (line 497).
  3. Count the loads from `self`-relative offsets inside the reader loop.
  4. Only if they are there, restructure, then run the quick benchmark.

### Idea 2. Starting and finishing a step with a record (a bundle of four small changes)
Measured cost [code: the task statement and log line 927]: `begin_step` is about 0.17 µs and `leave_split` about 0.27 µs, out of about 1.6 µs per step with a record. Steps with a record are about half the run time (log line 901).

**2a. One context lookup per step instead of two, with a cheap hash.**
- **Source.** T, the Matrix SDK case: data needed in the hot loop is "populated once before the sort ran". The fix was 78 times, from doing the per-item work once instead of repeatedly. The parallel here is weak: it is about doing a lookup once, not about memory.
- **Where.** `engine/src/sim2/split.rs`:
  - `contexts: HashMap<Meta, Context>` with the default SipHash, line 164 [code];
  - looked up in `begin_step` at line 275 (`entry(key)`), and again in `leave_split` at line 676 (`get_mut(&self.split.key)`) for the same key [code];
  - `Meta` (`engine/src/replay.rs`, lines 15–24) derives `Hash`, so a lookup feeds SipHash five calls: three `u32` and two `bool` [code];
  - `replay.rs`'s `Mix` hasher overrides only `write_u64`. Its `write_u32` falls back to the default, which goes through `write` byte by byte (lines 63–70) [code]. Moving `contexts` onto `Mix` as it stands would hash 14 bytes one multiply at a time.
- **Change.** Keep `contexts` as a `Vec<Context>` plus a `HashMap<Meta, u32>`, or keep the map and store the context's position. Remember the position in `begin_step` so that `leave_split` does not hash. Hash `Meta` as two `u64` with `Mix`, through a hand-written `Hash` impl or by keying on a packed `(u64, u64)`.
- **Why it might help.** It saves one SipHash-1-3 of about 14 bytes and makes the other cheaper. Perhaps 20 to 40 ns per step with a record, 1.5 to 2.5% of such a step [assumed: SipHash on short keys costs some tens of ns].
- **Why it might not.** Even at the top of that range it is about 1% of a run, under the 1.015 bar alone.

**2b. No `Mutex` and no channel poll at every step start.**
- **Sources.**
  - T, Matrix SDK: the sort "acquired a lock 322,042 times per pass". The fix needed "no lock access during the sort itself".
  - H, `/profiling_overhead`: an instrumented lock cycle costs 29–66 ns and a channel send/receive cycle 47–88 ns on an M2 Pro. Those are instrumented costs, an upper bound and not the bare cost.
- **Where.** `split.rs`:
  - `collect`, lines 396–409, runs at every `begin_step`, that is at every step replay misses: about 730,000 per 300 million ticks on day 4 part 2 (log line 881) [code].
  - Each call takes `self.split.workers` out of its `Option<Arc>`, calls `done.try_lock()` on a `Mutex<mpsc::Receiver<Done>>` (line 141), then `try_recv()` [code].
  - Only the main thread ever locks `done`. The `Mutex` exists only so that `Workers` can sit in an `Arc` inside a `Clone` struct [code: the workers use only `taken` and `finished`].
- **Change.** A worker increments an `AtomicU32` "records ready" counter when it sends a `Done`. `collect` does one relaxed load, and touches the lock and the channel only when the counter has moved. Alternatively, keep the `Receiver` outside the cloned `Split`.
- **Why it might help.** An uncontended lock and unlock plus an empty `try_recv` is perhaps 10 to 20 ns [assumed]. Over a few million step starts per long run that is tens of milliseconds.
- **Why it might not.** Under 1% of a run. std's `mpsc` has been crossbeam's implementation since Rust 1.67 [assumed], so swapping to crossbeam or flume would gain nothing.

**2c. `fits` without an early exit, so that it vectorises.**
- **Source.** T: "LLVM will vectorize straightforward loops over slices"; check whether it did before doing anything by hand.
- **Where.** `split.rs`, line 276: `fits` is `(0..words).all(|w| …)` over about 33 words (2,100 simulated signals), reading four arrays of a boxed `Record`: `start`, `known`, `first`, `known_first`, at lines 59–62 [code]. It is called for up to 6 records (`RECORDS`, line 96).
- **Why it might help.** For the record that fits, the loop always runs all 33 words. `.all` exits early, which usually keeps LLVM from vectorising [assumed]. An OR over all words with one compare at the end runs as NEON two words at a time, about 130 loads in about 20 cycles [assumed]. The records that do not fit exit early today, and a branch-free version would run them in full, so the order matters: the record used last is already first (line 677).
- **Why it might not.** Saving perhaps 10 to 20 ns of 170 is small. Records that do not fit would get dearer.
- **Variant.** Interleave the four arrays in one `Vec<[u64; 4]>` per record, which is T's "populated once" compact layout. The fit check then reads one stream instead of four heap blocks. Everything is in L1 already (attempt 21), so this is about instructions, not misses.

**2d. `leave_split`'s own lists.** Lines 651–653 choose `readers` or `direct` again inside the inner loop, the same pattern as `propagate_split`'s lines 537–538 [code]. LLVM probably hoists this [assumed]. Look at it in the same assembly pass as idea 1; it is not a separate attempt.

**Testing the bundle.** Make 2a and 2b one attempt and run the quick benchmark. If it lands under 1.015, run three full runs to the Breakpoints: those are where steps with a record dominate.

### Idea 3. hotpath as a one-off measurement: what it shows that sampling does not
- **What H measures** (from its pages):
  - **Functions:** calls, average, P95, total and share per instrumented function or block. The cost is about 40 ns per call, two clock reads and a queue push. There is a "count-only" mode at sampling rate 0, and rates of 0.01–0.1 are suggested for frequent calls.
  - **Allocations:** bytes per function, exclusive by default. This works through a counting global allocator that forwards to `System`, or to jemalloc or mimalloc if set.
  - **Threads:** CPU % per thread, peak and average, sampled every 250 ms, on macOS too.
  - **Locks:** wait time and hold time per lock, for std `Mutex` with no feature flag.
  - **Channels:** throughput, queue depth and peak, and latency, for std `mpsc` with no feature flag.
  - **CPU:** its CPU profiling is samply underneath.
  - **Its own comparison** (`/blog/sampling_comparison`): on CPU-bound code it agrees with samply, and its value is wall-clock and waiting time, which sampling misses.
- **Where it would add something here:**
  - **The jobs channel** (`split.rs`, line 364, `sync_channel(256)`; `ask`, line 387, `try_send`): a proof is silently not asked when the queue is full [code]. Channel queue depth and peak would show whether that ever happens.
  - **The done channel** (line 365): send-to-receive latency is how stale a record is when it arrives. That decides whether a faster proof (2b-v) is worth anything. A sampling profiler cannot see it.
  - **The worker's CPU %** puts a number on "one is never behind" (log line 903). It also shows how much of a core sits idle for anything speculative (v2.1 idea A4).
  - **Per-call P95 of `begin_step` and `leave_split`**: sampling gives the mean share, not whether 0.17 µs is a steady cost or a mean over a long tail. Possible tails, all [code]:
    - the first step of a context, which allocates two `Vec`s at line 298;
    - a `Job`, which allocates five `Vec`s at line 314;
    - growth of `contexts`.
  - **Allocation per function** answers the allocator question directly (2b-i).
- **Where it would not help.**
  - `propagate`, `feed` and `step_real`: 40 ns per call against a 70 ns tick and a 10–13 ns flip would swamp what is measured. The engine's own `--cfg profiling` counters and a sampler already cover these.
  - H's Callgrind instruction counts: Valgrind does not run on macOS on arm64 [assumed].
- **Cost.** A dependency behind a cargo feature. H says it costs nothing when the feature is off, and the macros are no-ops. `#[hotpath::main]` takes over the global allocator only with `hotpath-alloc`. Adding it is the user's call; I installed nothing.
- **Cheaper alternative without the crate.** In a `--cfg profiling` build:
  - an `AtomicU64` count of failed `try_send`s;
  - per `Done`, the step count at which its job was asked and at which it arrived;
  - a histogram of `begin_step` times with `mach_absolute_time`. H notes that this clock costs 6.1 ns against 17.2 ns for `std::time::Instant`.

### Idea 4. The schedule's modulo by a period known only at run time
- **Sources.**
  - F, the opening post: dividing by a non-power-of-two costs about 10 times multiplying by the reciprocal (2,658 ns against 232 ns, f64), and a mask beats `%` when the bound is 2^n − 1.
  - scottmcm (F): LLVM does this itself for divisions by constants, not by values known only at run time.
  - T: image-rs gained 5.9 times partly by "replacing integer division with reciprocal multiplication (the Granlund-Montgomery trick)".
- **Where.** `engine/src/sim2.rs`:
  - `Schedule::key`, lines 92–94: `(tick - lead - 1) % period`, with `period` a run-time `u64` [code].
  - Called by `step_real` at line 680 and twice through `Schedule::next` at line 98 (the second call is probably merged with the first [assumed]) whenever `tick == schedule_due`.
  - `meta()`, line 844, does `tick % clock_interval` and `schedule.key(tick)` again at every `intern` and every `begin_step` [code].
- **Change.** Keep the schedule's position as a counter that advances with `schedule_due`, as attempt 13 did for the clock and timer due ticks. `meta()` would read the kept positions. Taking the period from the board at run time is allowed by the project's rule; it is found, not hand-marked.
- **Why it might help.** A 64-bit `udiv` takes some cycles on Apple cores [assumed: under 10]. It costs nothing to remove.
- **Why it might not.** At most a few divisions per simulated tick that has a schedule flip, and per step: a fraction of a percent. Attempt 13 already took the large fixed work out. Low; only worth folding into another attempt.

### Idea 5. Signed and unsigned arithmetic in the counts: checked, nothing to do
- **Source.** F, posts 2 to 4: halving a signed integer needs extra instructions; with `u32`, `/ 2` and `>> 1` cost the same. Use signed types only when negative values are needed. scottmcm adds the pre-RFC for `unchecked_*` arithmetic.
- **Where, all [code]:**
  - the counts are `u16`, with `wrapping_add(u16::MAX)` for −1;
  - `Op::Xor => active % 2 == 1` on a `u16` (`evaluate`, line 466) is an AND with 1;
  - indices are `u32` widened to `usize`.
- **Verdict.** Nothing here pays the signed-arithmetic cost. `unchecked_add` (stable since 1.79 [assumed]) would add nothing over `wrapping_add` for this code. Listed so that the question is closed.

## 2b. The topics you asked about that no source covers

The sources say nothing about these. They are my reasoning against the code, with no source behind them.

**(i) Allocator: mimalloc or jemalloc.**
- **Sources.** H's guide says only that H "wraps the allocator of your choice". T and F do not mention allocators.
- **Code [code]:**
  - The tick allocates nothing (attempt 1).
  - `begin_step` on the path with a record allocates nothing: `scratch` and `first_list` are reused, lines 241–261 and 321.
  - `leave_split` reuses `sources`.
  - On the main thread, allocation happens:
    - at a context's first step (line 298);
    - in a `Job`, five `Vec`s of about 33 words each (line 314), a few thousand per program (3,578 proofs per 300 million ticks, log line 881);
    - in `Table::intern` and `record` growth (`replay.rs`, lines 160–164, amortised doubling);
    - in `HashMap` growth.
  - Records are allocated on the worker (`make_record`, lines 814–881) and freed on the main thread when pushed out (`install`, line 417): a free across threads, but rare.
- **Estimate.** Thousands of allocations per second at most, against a run of seconds. A faster allocator might save milliseconds. The large `Vec`s of the replay table regrow through `realloc`, which macOS serves by remapping pages for large blocks [assumed].
- **Test before any attempt.** Count the allocations on the main thread during a 20-million-tick run, with a small counting `#[global_allocator]` in a profiling build or with hotpath-alloc. If it is under about 10^5, drop the idea. If it is much higher, something allocates where the code says it does not, and that is the finding.

**(ii) The default hasher on `HashMap<Meta, Context>`.** Covered in idea 2a. The gain is small, and the second lookup is the larger part of it.

**(iii) `Vec<Box<Record>>`.**
- **Code [code].** The `Box` makes `records.remove(found)` (line 280) and `insert(0, …)` (lines 414 and 677) move 8-byte pointers. A `Record` itself is about a dozen `Vec` headers.
- **Cost.** The `Box` adds one pointer chase per record in `fits` and per tick in `mode_tick` (`record.timed`, line 480), and all of it is in L1. Removing the `Box` would make the shifts move about 300 bytes per record; that is no win.
- **What matters instead.** The `Vec`s inside a record: 2c's interleaving.

**(iv) `#[inline]`, `#[cold]`, `likely`/`unlikely`.**
- **Sources.** None of the three gives advice on these. H's CPU page notes in passing that its instrumentation forces `#[inline(never)]` onto measured functions, which changes what it measures.
- **What the code shows [code]:**
  - `mode_tick` (`split.rs`, line 469) is `#[inline(never)]` and is called after every tick in a mode other than `NORMAL` (`sim2.rs`, line 724).
  - The main thread with a worker never enters `PROOF`: `begin_step` calls `start_proof` only when `threads == 0` (lines 319–330). So on the main thread `mode_tick` is always the `SPLIT` path: about 18 calls per step with a record, the common case now (93% of missed steps on day 4 part 2).
  - `leave_split` (line 609) is `#[cold] #[inline(never)]`, yet it runs once per step with a record and costs about 0.27 µs.
- **Idea.**
  - Split `mode_tick`: inline the `SPLIT` branch into `step_real` and keep `proof_tick` out of line.
  - Remove `#[cold]` from `leave_split`, keeping `inline(never)` if you like.
  - What `#[cold]` does to the cold function's own code generation, beyond the caller's branch weights, I do not know [assumed: possibly optimised for size]. PGO's real counts may already override it.
- **Test.** Two attributes changed, then the quick benchmark with a PGO build, since PGO decides inlining in the build that is used.
- **`likely` and `unlikely` hints.** The stable form is the `cold` attribute or a cold path. I did not check which hint APIs rustc 1.100 has stabilised.

**(v) `Vec<bool>` against bitsets in the proof.**
- **Code [code].** `unknown`, `ever`, `before` (lines 182–187), and `changed` and `queued` as `Vec<u64>`, live on the worker's copy. The proof runs there, and the worker is never behind (log line 903).
- **Verdict.** Bitsets would make records arrive sooner, not make the main thread faster. That matters only if idea 3 shows records arriving late enough that steps are simulated whole while waiting. Neither the 35 µs proof nor its restore (`prove`, lines 432–438, which recounts every simulated entity) is on the main thread's path.

**(vi) Prefetching.** None of the sources mentions it. The data the tick touches fits in L1 (attempt 21, v2.1 step 4). Nothing to prefetch.

**(vii) Bounds checks in `step_real` and `propagate`.** Already removed (attempt 14, `g`/`gm` at `sim2.rs` lines 19–31) [code]. `split.rs`'s `leave_split` (lines 651–658) and `count_fixed` (lines 593–599) still index with checks: `lists.start[e]`, `lists.items[i]`, `self.cell[reader]` [code]. They run once per step, not per tick: perhaps a few ns each. Fold into idea 2 if it is built.

**(viii) Branch prediction in `feed`'s `match op`** (line 556). F's ring counter (an `if` at 469 ns against a mask at 56 ns) suggests a branch-free wake test per op, from a table. But F's own replies warn that the two versions there did different things. Attempts 25 and 26 showed that anything added to this loop costs more than it saves, and the log says the branches are predicted. Not recommended.

## 3. Advice from the sources that is done or does not apply

- **Profile first** (T, step 1; H throughout). Done: `--cfg profiling` counters plus a sampling profiler since v2.1 step 0.
- **Layout before SIMD** (T, step 2; the 64-byte struct in the Matrix SDK). Done for the tick: an 8-byte `Cell` per entity (attempt 20). Renumbering for locality was tried and gave nothing (attempt 21).
- **Integer accumulators instead of floats** (T, image-rs). The engine uses no floats in its hot paths [code].
- **Explicit SIMD with `core::arch`, `std::simd`, `wide`, `pulp`** (T). There is no wide numeric loop. The tick is 3 to 7 scattered flips. Only `fits` (2c) and `Table::hash` are word loops, and `hash` already runs four lanes (`replay.rs`, lines 121–139) [code].
- **The FFI boundary cost** (T). There is no FFI in the engine.
- **C++ libraries: BLAS, Eigen** (T). Not relevant.
- **A shift instead of a signed division by 2; a mask instead of a modulo** (F). The counts are unsigned. The only run-time modulos are idea 4's.
- **Multiplying by the reciprocal for floats** (F). There are no float divisions.
- **The pre-RFC on `unchecked_*`** (F). `wrapping_add` and unchecked indexing already cover it.
- **Fixing an SQL N+1, parallel HTTP calls, I/O buffering, `Arc<str>` instead of cloning a `String`, compiling a regex once** (H's guide). None of these is in the engine.
- **H's fast clock** (`mach_absolute_time`, 6.1 ns against 17.2 ns for `Instant`). `Instant` appears only in `main.rs`'s progress output (lines 605–629), not in a hot path [code].
- **Holding a lock across slow work** (H's guide, a write guard held across an HTTP call). Neither lock is held across work: `collect` holds `done` only while draining, and the worker holds `taken` only inside `recv` (`split.rs`, line 376) [code].
- **Already done and not proposed again:** fat LTO, one codegen unit, `panic=abort` (`Cargo.toml`), `target-cpu=native` (`.cargo/config.toml`), PGO (`tools/build-pgo.sh`), unchecked indexing, flat lists, no allocation in the tick, cold paths (attempt 16), a custom hasher for the replay table (`Mix`), one worker thread.

## 4. What I could not read or check

- **Every page was fetched.** No archived copy was needed. The pages came through a fetch tool that summarises with a small model. I fetched T a second time for verbatim sentences on a keyword list, and the two readings agree. Exact wording in sections 2 and 3 may be paraphrased; the numbers are as the tool returned them.
- **Not opened:**
  - H's pages on SQL, HTTP, axum, Tokio, Prometheus and Grafana, I/O, debug macros, MCP, cloud, Callgrind, Criterion and CI. None is about this kind of program.
  - The links inside F (playground and Compiler Explorer snippets, the pre-RFC). The posts say what they show.
  - T's two newsletter links. The article states the techniques itself.
- **Nothing was built, run or measured.** I had no cargo, by the rules of the task. Every cost estimate marked [assumed] needs a measurement:
  - the cost of SipHash on `Meta`;
  - the cost of an uncontended `Mutex` and `try_recv`;
  - whether `.all` blocks vectorisation;
  - whether `propagate` reloads through `self`;
  - whether std's `mpsc` is crossbeam's code;
  - Valgrind on macOS arm64;
  - what `#[cold]` does to the cold function's own code.
- **Two numbers come from the task statement, not from my reading of the logs:** the 0.17 µs and 0.27 µs per call. The log gives 0.17 µs per start (line 861) and "a third of 1.6 µs" for start and finish together (line 927).
