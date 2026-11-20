# Research 3: SIMD and vectorisation in the v2 engine

Written 2026-11-20 by one agent (Opus 5.5), at the user's request. Code read at commit `01fee77` (attempt 28). Nothing in the engine was changed. Loop shapes were copied into a scratch crate under `/tmp` and compiled with the engine's settings (`-C target-cpu=native`, release, rustc 1.100.0, LLVM 23.1.5); the engine's own release binary (`engine/target/release/vcb`, built 2026-11-20) was disassembled to check that the engine's loops look the same as the copies.

Tags used below: **[verified: code]** read in the source; **[verified: asm]** seen in compiled output; **[verified: bench]** timed in the scratch crate (single runs, 20 million calls, nothing else heavy running; numbers within roughly 10%); **[inferred]** reasoning, not measured; **[source]** from a source in section 4.

## 1. The answer in five lines

1. **The tick itself (`propagate`, `feed`, `evaluate`, about 88% of run time) has nothing for SIMD.** Each tick touches 6 to 8 scattered 8-byte cells and about 24 reader updates through lists about 3 long. There is no gather or scatter on this machine, and two updates in one tick can hit the same count. Bit-parallel simulation across gates, test patterns or time does not fit either: its preconditions (levelised, zero-delay or acyclic, everything evaluated) do not hold, or it does more work than the event-driven tick.
2. **The packed-state loops do have something, and it is small.** `begin_step`'s record check is scalar in the engine, about 25 instructions per word with six bounds checks. A branch-free version that LLVM vectorises is about 3 times faster (14 ns against 42 ns for 50 words in the copy). A record's net change stored as an XOR mask can be applied to the packed words in about 6 ns.
3. **The largest packed-state item is not SIMD.** `Table::hash` keeps its four "parallel" lanes in a stack array, so every word goes through a store and a reload (seen in the engine binary). Rewritten with the lanes in registers it is 3.3 times faster (10 ns against 34 ns for 50 words) and gives exactly the same hash values. A NEON AES-round hash would take 4.6 ns, but its values would be different.
4. **The ceiling:** these together save roughly 40 to 100 ns per simulated step, against 1.6 µs for a step with a record and 3.4 µs for a whole step. That is **about 2% to 5% of the run at most**, and probably 2% to 3%. That is above the 1.015 bar only if the parts add up.
5. **On this machine, stable Rust gives you NEON (128-bit) through `std::arch::aarch64` and through auto-vectorisation, and that is all.** `std::simd` is still unstable, the SVE intrinsics and the `sme` target feature are unstable, and the M5 has SVE only in streaming mode (the SME mode) anyway.

## 2. The five questions

### Q1. The event-driven tick: does not help

**Code** [verified: code]:
- `engine/src/sim2.rs`: `step_real` (640–726), `evaluate` (456–474), `propagate` (498–537), `feed` (550–582), `flip_bit` (541–546).
- `Cell` is 8 bytes: `count: u16`, `threshold: u16`, `op: u8`, then three bools (35–48).

**What a tick does.**
1. Evaluate. It loops over `dirty`, 6 to 8 entries. For each it reads one cell and does a `match` on `op` with a compare, then pushes the gate to `flips` if the output changed.
2. Apply. It loops over `flips`: toggle `on`, XOR one bit into `packed`, OR in `pin_role`.
3. `propagate`. For each flip it adjusts the counts of the traces it writes. Then for each trace whose state changed, it calls `feed` on every reader. `feed` does a read-modify-write of one count, a `match` on `op`, and possibly a push to `dirty`.

**Why SIMD has nothing to work on here:**
- **Too few lanes.** A batch is 6 to 8 gates per tick, and each reader list is about 3 entries. A 128-bit register holds two 8-byte cells, so a batch fills four registers once per tick.
- **No gather or scatter.** NEON has neither [verified: asm and target features: no `sve` in non-streaming mode, see section 3]. Loading 8 scattered cells into vector lanes takes 8 scalar loads plus lane inserts, which is the same 8 loads the scalar code already does. After that, the vector work would replace about 5 to 8 scalar ALU instructions per gate, but it adds extracting a mask and compacting the flips list.
- **The copy did not vectorise either.** I compiled a branch-free gate evaluation over a batch (`eval_batch` in the scratch crate) and LLVM emitted no vector instructions for it [verified: asm].
- **Writes collide.** In `feed`, two readers in one batch can be the same gate: a gate fed twice in one tick, up then down. A vectorised scatter would need conflict detection, like AVX-512 CD, which ARM does not have. The updates are also serial through memory: a later `feed` reads the count an earlier one wrote.
- **The cost is not memory.** Attempts 20 and 21 already showed that everything is in L1. What is left is instruction count and branches per flip, and SIMD only cuts the first, on code that is already short.

**NEON table lookups (TBL).** A byte-shuffle lookup can evaluate up to 16 mixed gates at once. You put the sum of each gate's inputs plus an offset for its operation in a byte lane and use it to index a 16-byte truth table. Apple's patent US10331830 describes exactly this [source]. It needs the gates' inputs already sitting in lanes, which means levelised evaluation of many gates at once (the patent sorts the netlist into levels). v2 has 6 to 8 gates scattered over 2,100. For a single gate a scalar 16-byte lookup table does the same job, so TBL itself buys nothing.

**Branch-free evaluation (scalar).** This is the one related idea that is not dead.
- `evaluate`'s `match` and `feed`'s `match` (556–568) branch on `op`, which depends on the data. Whether they mispredict has never been measured (idea C5 of research 2).
- A scalar branch-free form is possible. Give each cell a 2-bit class (threshold, parity or latch) and an invert bit. Then compute `result = select(class, count >= threshold, count & 1, on ^ toggle) ^ invert`, and in `feed`, `wake = (class==T & max(old,new)==threshold) | (class==P) | (class==L & now)`.
- Edge case: AND and NAND with no inputs are both off (465). That needs its own threshold encoding.
- This is not SIMD.
- Attempts 25 and 26 found that anything added to the loop over readers costs more than it saves. This idea replaces a branch rather than adding work, which is different, but the risk is the same kind.

**Verdict: does not help (SIMD).** Branch-free scalar: **might help, a few percent at most, only if mispredictions are real.**
- Cheapest experiment: count branch mispredictions per flip with Instruments' CPU Counters on a run with replay off (C5).
- Only if they are a large share of cycles, try the branch-free `feed` as a quick-benchmark attempt (a few hours).

### Q2. The packed-state operations: helps a little, and the best item is not SIMD

All timings are for 33 and 50 words, the sizes of the user's CPU boards [verified: bench]. "Engine shape" means what the engine binary contains [verified: asm].

| Operation | Code | Engine shape | Copy as written | Rewritten | Gain per call (50 words) |
|---|---|---|---|---|---|
| Record check `fits` | `split.rs` `begin_step` 276 | Scalar, early exit per word, 6 bounds checks and the record's 6 `Vec` pointers reloaded every word: about 25 instructions per word | 28 / 42 ns | Branch-free OR-fold, vectorised by LLVM (`ldp q`, `eor.16b`, `and.16b`, `orr.16b`): **9.8 / 14.3 ns**. Early exit per 8-word chunk: 23 / 33 ns. Record stored interleaved (`[start, known, first, known_first]` per word): 14 / 22 ns | about 28 ns per record checked to the end |
| Replay hash | `replay.rs` `Table::hash` 121–139, inlined into `Table::intern` | **The four lanes live on the stack** (`str`/`ldr` through `[sp, #0x70]` at every word, at `0x10006d308` in the binary): one chain through store-to-load forwarding, not four | 22 / 34.5 ns | Lanes in registers (`chunks_exact(4)` plus the rest in order), **same values** (asserted for every length 0 to 50): **6.3 / 10.4 ns**. NEON AES rounds (`vaeseq_u8`/`vaesmcq_u8`, two 128-bit lanes, different values): 3.6 / 4.6 ns | about 24 ns (registers), about 30 ns (AES) per intern |
| Node equality | `replay.rs` `intern` 150 (`b == bits`) | `memcmp` from libSystem, already vectorised | 5.7 / 7.7 ns | none | none |
| Context learning | `split.rs` `begin_step` 302–307 | Not vectorised: indexed by `words`, which blocks the bounds-check hoisting | not timed | `zip` over slices would vectorise (inferred from the `fits` case) | small; only on steps that fit no record |
| Net change applied | `split.rs` `leave_split` 617–625 | Scalar: per entity `on ^= true` plus `flip_bit` (a `slot` load and a read-modify-write of `packed`) | – | XOR mask over the packed words: 5.1 / 6.1 ns, vectorised | about the `flip_bit` part of 32 to 47 entries, perhaps 15 to 30 ns [inferred] |
| Count changes applied | `leave_split` 622–625 | Scalar scattered `u16` adds, 61 to 80 of them | – | No gather or scatter on NEON | none |
| Restore by difference | `sim2.rs` `restore` 907–920 | Scalar XOR, then a trailing-zero walk; CSSC `ctz` is used with `target-cpu=native` | – | XOR could be vectorised to skip equal words, but the work is in the per-bit count updates (scattered) | none worth having |
| Restore copy | `restore` 925 | `copy_from_slice`, which is `memcpy` | – | – | none |

What this adds up to [inferred from the timings and the measurements in the brief]:
- **`fits`.** A step that finds a record checks at least one record to the end, usually the first one, since the last used record is kept in front. A step that fits none checks up to 6, each stopping at its first differing word, which can be anywhere. Saving: about 30 to 55 ns per split step, out of 1.6 µs, so **2% to 3% of a split step**.
  - The engine's version reloads more than my copy does, so its real cost is probably above the copy's 42 ns.
  - The change is about five lines: compute `acc |= …` over `zip`ped slices and test `acc == 0` at the end. The early exit is lost, but at these sizes the full vector pass costs less than the scalar loop's first third.
- **The hash.** It runs once per event in a simulated stretch (`Engine::intern`, `sim2.rs` 849–878, once per step). Saving about 24 ns per step: **about 1.5% of a split step and 0.7% of a whole step**.
  - Same hash values, so saved replay tables would still match, except that the fingerprint covers the build and makes them stale on any rebuild anyway.
  - It is a register-allocation fix, not SIMD. The source comment says the four lanes "do not wait for each other"; in the compiled code they do.
  - AES rounds would save about 6 ns more per call. That is not worth a new hash function, which also needs a check of its distribution.
- **Net change as XOR masks.** The question was whether to store a record's net change as XOR masks over the packed words plus the list. It saves only the `flip_bit` half of each net entry, because `cell.on` still has to be toggled entity by entity: the tick reads `on` from the cell, not from `packed`.
  - Perhaps 15 to 30 ns of the 0.27 µs `leave_split`: **about 1% to 2% of a split step**.
  - The mask is one more `Vec<u64>` per record, about 400 bytes against the record's 8 KB.
  - The step that ends early (offset below `length`, 627–634) does not use `net`, so it gains nothing.
- **Already vectorised by LLVM** [verified: asm]: `memcmp`, `memcpy`, the `vec![0; n]`/`resize` memsets (the one in `first_flips`, 243, is a 400-byte memset), and the `!w` map at 352 (`known`).

**Verdict: might help, about 2% to 5% of a split step together**, which is a smaller share of the whole run, since split steps are part of the 88% that is simulated.

Cheapest experiment: one attempt with three small changes:
1. `fits` as a `zip`-fold.
2. `hash` with the lanes in registers, values unchanged.
3. The context map's hasher (see Q5).

Then the quick benchmark. The XOR mask for `net` is a second, separate attempt, because it changes `Record`.

### Q3. The proof on the worker as bit-planes: does not help (off the main thread)

**Code** [verified: code]: `split.rs` `proof_tick` 690–807, `start_proof` 334–359, `make_record` 810–883, `prove` 425–457. The proof state is `Vec<bool>` (`unknown`, `ever`, `before`) plus `Vec<u64>` tick stamps (`changed`, `queued`), indexed by entity. The work is event-driven: only the queued readers are looked at (`queue`, `next`).

**Could unknown/value be two bit-planes so that a level is evaluated with word operations?** Yes, in the textbook sense. Dual-rail three-valued simulation with 2 bits per signal is how parallel fault simulators encode X [source: the parallel fault simulation lecture notes; inferred for this engine]. But the proof is not a levelised pass. It follows a few dozen changing gates per tick with feedback, like the tick itself. For word operations to replace it, the gates' inputs would have to be gathered into bit positions, which brings back "evaluate everything", the approach closed in v2.1 ("word-wide simulation of bit slices"). Packing `unknown` and `ever` into bitsets would help only `make_record`'s scans over `live_list` (816–830). Those run once per proof.

**What a faster proof would buy.**
- The main thread never waits for it. Attempt 28 found that one worker is never behind, and that more workers gain nothing.
- A faster proof only shortens the time from the moment a proof is asked for to the moment the record can be used. That counts only if the same context comes back while its proof is still running (`context.asked` true at 311) and there is no older record that fits.
- With the proof at about 35 µs (ten steps), that is at most a few steps per proof. Proofs are asked for on 3% to 11% of the steps that miss.

**Verdict: does not help.** If anything here matters, it is latency, and the cure for latency is a cheaper proof algorithm, not SIMD.

Cheapest check (no build): count the steps that begin while their context has `asked == true` and no record fits. If that is under 1% of split steps, the question is closed.

### Q4. Bit-parallel across time (C3) or across data variants: does not help

**What published bit-parallel simulators assume:**
- **Parallel-pattern simulation** (PPSFP and its family) puts 32 or 64 independent input patterns into one word and evaluates every gate of a combinational, levelised circuit once per word. It was extended to synchronous sequential circuits by treating the flip-flops as cut points between cycles [source: fault simulation lecture notes and theses].
- **Maurer's "parallel technique"** for unit-delay compiled simulation gives each net an n-bit field holding its values over n time steps. It is defined "for acyclic circuits": a combinational block within one clock period. Reported speed: about 10 times interpreted event-driven simulation on ISCAS-85 [source: abstract and summary only].
- **Compiled-code simulation in general** is "limited to synchronous circuits with zero-delay or unit-delay" and requires the gates to be levelised [source: Wang and Tropper 2006, read].
- **Compiled-code event-driven simulators** (Lewis 1991, Wang and Maurer 1990) compile per-gate code but keep an event-driven kernel, with one gate value at a time [source: as above]. The SIMD versions found (the Apple patent, Glasgow's ZSIM) evaluate gates level by level [source].
- **GSIM, ESSENT and Redpiler.** GSIM's bit-level work is about activity tracking (splitting nodes per bit), not about packing values into words. ESSENT is about partition activity. MCHPRS's Redpiler "Direct" backend is an interpreter over a precomputed graph with a "rotating queue of queues" for its tick delays, with no mention of SIMD or bit-packing [source: the GSIM paper and the Redpiler doc, both read].

**Do the preconditions hold here?** No.
- The circuit is sequential at gate level. Latches toggle on rising edges, and there are feedback loops through ordinary gates, with no clock that cuts them into combinational blocks.
- Every gate has a delay of one tick, so there is no zero-delay level order.
- Activity is 0.3% of gates per tick. Pattern parallelism evaluates every gate.

**Across time (C3), with feedback.** One word holds a signal's value over 64 ticks; you iterate until the words stop changing (waveform relaxation).
- In v2 every flip at tick t that is not from the schedule is caused by a flip at t−1, because the delay is one tick. So the chain of cause and effect inside a busy stretch is as long as the number of consecutive busy ticks, about 41 per step.
- Relaxation fixes at least one more tick per pass, so it needs about that many passes. Each pass re-evaluates the gates on the frontier, which is at least the work of the flips themselves.
- On top of that it adds word operations, shifts and the convergence test. Time-parallelism pays only when a gate flips many times in a window while its inputs were settled early. That is the self-running clock part, which v2 already plays from a schedule (attempt 11).
- [inferred]

**Across 64 data variants.** Steps are sequential: each starts from the one before, and the next VMem word is known, not guessed. Step 5's results argue against guessing ahead (A4).
- The one place where many variants of a context's start exist is the proof.
- Simulating, say, the last 64 starts of a context bit-parallel would show which signals were fixed in those 64, but would not prove it for unseen data. The record would be unsound, which the engine's rules rule out.
- Three-valued simulation is the sound form of the same thing, and it is event-driven (Q3).
- [inferred]

**Verdict: does not help.**

Cheapest check if anyone wants a number instead of the argument: from a recorded flip stream of simulated steps, compute per 64-tick window (a) flips per distinct flipping entity and (b) the longest chain of flip, reader flips one tick later. If (a) is near 1 to 2 and (b) is near the window's busy ticks, C3 is closed. One estimator in `examples/`, an hour.

### Q5. Other places

- **Scanning the queue.** There is no scan. `dirty` is an exact list (`feed` 578–581), so there is nothing like GSIM's "test 8 active bits at once" (C6 in research 2) [verified: code]. Does not help.
- **Popcount and trailing-zero walks** (`restore` 909–911, `proof_tick` 702–705 and 787–790, `make_record`). With `target-cpu=native` (apple-m5) LLVM already emits the single CSSC `ctz` instruction [verified: asm in the copy]. Does not help.
- **Copies in the worker's jobs** (`begin_step` 314). Two `to_vec`s of 33 to 50 words, two `clone`s of the unknown vectors and a list. Each is a `memcpy` of about 400 bytes, a few ns, made only when a proof is asked for. Does not help.
- **`first_flips`** (240–263). A 400-byte memset plus evaluating `dirty`. Already vectorised where it can be.
- **`show`** (829–833), **the full recount in `restore`** (896–903) **and `prove`** (432–438). These run at `sync`, once after a node is cleared, and on the worker. Off the hot path.
- **Not SIMD, but in the same function as Q2's items:** the `contexts` map (`split.rs` 164) uses std's default `RandomState`, which is SipHash-1-3. It hashes a `Meta` of about 14 bytes once per `begin_step`, and the binary shows the SipHash rounds inlined there. A multiply-based hasher like `replay.rs`'s `Mix`, with `write_u64` on a packed `Meta`, would save perhaps 10 to 20 ns per step [inferred]. The map's lookup itself, `hashbrown`, already uses NEON group probing.

**Verdict:** nothing here except the `contexts` hasher, which belongs in Q2's attempt.

## 3. What stable Rust offers on this machine

Checked on this Mac with rustc 1.100.0 stable (aarch64-apple-darwin, LLVM 23.1.5) and `sysctl` [verified].

| What | Status |
|---|---|
| NEON intrinsics, `std::arch::aarch64` (`vld1q_u64`, `veorq_u8`, `vqtbl1q_u8`, `vaeseq_u8`/`vaesmcq_u8`, `vmull_p64`, …) | **Stable.** A test file with AES and TBL intrinsics compiled. NEON is always on for aarch64. |
| Auto-vectorisation to NEON | **Works** for simple loops without early exit or bounds checks inside: `fits_fold`, `xor_apply`, `negate`, memset and memcpy. It **does not** vectorise early-exit loops that keep bounds checks (`fits_early`), loops indexed by an unrelated length (`learn`), or gathered cell loads (`eval_batch`). |
| `std::simd` (portable SIMD) | **Unstable** (`error[E0658]: use of unstable library feature portable_simd`), nightly only, tracking issue #86656 |
| SVE intrinsics (`svptrue_b64` …) | **Unstable** (`stdarch_aarch64_sve`) |
| `#[target_feature(enable = "sme")]` | **Unstable** ("the target feature `sme` is currently unstable") |
| Hardware | Apple M5: `FEAT_SME`, `FEAT_SME2`, `FEAT_SME2p1`, maximum streaming vector length 64 bytes. No `FEAT_SVE` in `sysctl`: SVE is only available in streaming mode, as on the M4 [source], and switching with `smstart`/`smstop` costs enough that only long blocks of matrix work pay for it [source]. Also present: AES, PMULL, SHA3, CRC32, dotprod, i8mm, bf16, CSSC. |
| `target-cpu=native` | Resolves to `apple-m5`. It does not turn on `sve` or `sme`, but it does turn on CSSC (scalar `ctz`/`cnt`) and AES. |
| Caches | 128-byte cache lines; 128 KiB L1 data cache on the performance cores, 64 KiB on the efficiency cores |

So in practice: NEON at 128 bits through auto-vectorisation or `std::arch`. No gather, scatter or compress (the SVE features), and no conflict detection.

## 4. Sources, and what was read of each

- **Apple patent US10331830, "Heterogeneous logic gate simulation using SIMD instructions"** (patents.google.com/patent/US10331830). The first 100,000 of 145,000 characters, through a summarising fetch. Read: one byte lane per gate, inputs summed plus an offset for the operation, used as a byte-shuffle index into a 16-byte truth table; gates sorted into levels; no measured speedups.
- **MCHPRS `docs/Redpiler.md`** (github.com/MCHPR/MCHPRS). The whole doc, through a summarising fetch. Read: a graph compiler with passes; the Direct backend interprets the graph; "rotating queue of queues" for the tick delays; no SIMD or bit-packing mentioned; no JIT.
- **GSIM, arXiv 2508.02236 (HTML v1).** Fetched, with a question on SIMD. Read: no SIMD or vector instructions; "bit-level" means node splitting by data flow; 8 active bits tested in one condition; 82.26% of branches check active bits; activity factor 4.61%.
- **Wang and Tropper, "Compiled Code in Distributed Logic Simulation", WSC 2006** (cs.mcgill.ca/~carl/cidvs.pdf). Pages 1 and 2 read. Read: oblivious compiled simulation needs levelised gates and "is limited to synchronous circuits with zero-delay or unit-delay"; compiled-code event-driven simulation (Lewis 1991; Wang and Maurer 1990) keeps an event-driven kernel.
- **Maurer, "Two new techniques for unit-delay compiled simulation", IEEE TCAD 1992.** Search snippets only (Baylor repository record, Bohrium summary). The Baylor page returned 403. Claimed: an n-bit field per net, bit-parallel evaluation, for acyclic circuits; about 10 times interpreted event-driven simulation on ISCAS-85. Not read in full.
- **Parallel-pattern fault simulation.** Search snippets only: lecture notes (NPTEL, NYCU), a Virginia Tech thesis, an O'Reilly EDA book chapter. Claimed: the number of patterns per word follows the machine word; for combinational circuits; extended to synchronous sequential circuits.
- **Glasgow ZSIM (Chimeh and Cockshott)** and the **multi-processor gate simulator patent US8738349.** Search snippets only. Claimed: SIMD gate simulation level by level; oblivious evaluation inside clusters with event scheduling between clusters.
- **Rust `std::simd` docs and tracking issue #86656.** Search snippets. The status was then confirmed locally by compiling.
- **M4 SME exploration (github.com/tzakharko/m4-sme-exploration), Jena SME microbenchmarks, PMBS slides.** Search snippets. Claimed: M4 has SME2 and SVE only in streaming mode; mode switches are costly. The M5's flags were checked locally.
- Repository files read: `engine/src/sim2.rs` (whole), `engine/src/sim2/split.rs` (whole), `engine/src/replay.rs` (whole), `specs/vcb-engine-v2.1.md` (whole), `specs/vcb-engine-v2.1-research2.md` group C.

## 5. What I could not check

- **Nothing was timed inside the engine.** The per-step savings come from scratch-crate copies, whose `fits` is leaner than the engine's, and from the brief's step costs (1.6 µs, 0.17 µs, 0.27 µs). How much of `begin_step`'s 0.17 µs is `fits`, the SipHash, `first_flips` and `collect` was not measured.
- **The binary may be a plain build.** `target/release/vcb` was disassembled as found. It may not be the profile-guided build. The hash loop's spill to the stack is a code-shape issue, so it is very likely the same in a PGO build, but that was not checked.
- **Branch mispredictions in `feed` and `evaluate`** were not measured. The branch-free idea in Q1 depends on them.
- **How often a context comes back while its proof is pending (Q3)** was not counted.
- **The C3 window statistics (Q4)** were not computed. The verdict rests on the one-tick-delay argument.
- **Maurer's 1992 and Lee and Maurer's 1996 papers** were not read in full. The "acyclic" precondition is from the abstract.
- **The AES hash's distribution** was not tested. Only its speed was.
