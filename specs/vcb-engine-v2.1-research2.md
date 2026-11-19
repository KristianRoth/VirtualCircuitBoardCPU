# Second research pass: ideas for a faster tick-exact gate simulator

Written 2026-11-19. Scope: the angles the first pass left empty (Hashlife, incremental hashing, compiled traces, event queues, GSIM and other simulators, time-parallel methods, symbolic and abstract representations), plus anything else that fits.

## How to read this

Every idea has two tags.
- **Source support**: *verified* (I read the relevant part of the source), *claimed* (I saw an abstract, a summary or a snippet but could not read the passage), or *our own* (a combination or extrapolation; no source says it works here).
- **Fit**: *plausible*, *long shot*, or *probably not* (kept so it is not searched again).

The numbers used to judge bookkeeping, all from the project's own files:
- A simulated tick costs roughly 60 to 80 ns. A flip costs about 10 to 13 ns.
- A step (memory event to memory event) is about 156 to 163 ticks. When it is simulated, about 41 of those ticks are simulated and the rest are jumped as idle.
- A simulated step makes about 290 to 340 flips. 62% to 70% of them are the same (signal and tick offset) in every execution of the same context. There are 460 to 1,350 contexts per program.
- 88% of the run time is in the simulated ticks. `propagate` is about 60% of the time.
- The state is about 2,100 to 3,100 live signals, packed into about 33 to 50 64-bit words, kept up to date at every flip.
- Restoring a node already works by XOR of the packed states and count updates for the differing bits only (`restore` in `sim2.rs`).
- The machine is an Apple M5 (arm64). That rules out x86-only tricks such as AVX-512 gather and conflict detection.

**One measurement that many ideas below need and nobody has taken:** the *net* change of a simulated step. That is the number of state bits that differ between the start and the end of a step (popcount of start XOR end), against the 290 to 340 flips inside it. Step 8 of the v2.1 file closed read-set replay partly because "a step changes about 320 signals and putting them into the circuit costs what simulating them costs". 320 is the flip count. If the net change is much smaller (signals that rise and fall again inside the step do not count), every idea that "applies a step's result" instead of simulating it gets cheaper by that factor. I call this **measurement N** below. It is a counter of a few lines in an existing estimator such as `control_estimate.rs`.

---

## Group A. Using the per-context regularity

These are the ideas aimed at the measured fact that two thirds of the flips in a simulated step repeat in every execution of the same context.

### A1. Prove which flips of a context are fixed, with ternary (0/1/X) simulation

- **What it is.** Take the start state of a step. Set every bit that is not the same in all observed starts of that context to X (unknown). Simulate the step in three-valued logic (AND with a 0 input gives 0 even if the other input is X; a toggle latch whose input edge is X becomes X). Every signal that stays 0 or 1 at a tick is proven to have that value at that tick for any data. This is a binding-time analysis, done on the unrolled step instead of on program text: X is "dynamic", 0/1 is "static".
- **Where it comes from.** Ternary simulation and its covering property: Bryant, ISMVL 1994 ("any binary values resulting when simulating patterns containing X's would also result when the X's are replaced by any combination of 0's and 1's"). Binding-time analysis splitting "rt-static" from "dynamic": Facile (PLDI 2001). The combination is our own.
- **What is reported.** Bryant: used to verify RAM circuits of up to 4,096 bits with one ternary run instead of enumerating states. No speedup figure for simulation; it is a verification method.
- **Why it might help here.** Step 8 measured that 62% to 70% of flips are the same in every *observed* execution of a context. That is an observation, not a proof, so no engine can skip those flips safely. Ternary simulation turns the observation into a proof, per context, without any board knowledge. It is also the estimator every idea in this group needs: it says how many of the 2/3 survive as provably static.
- **Why it might not.** Ternary simulation is pessimistic. Reconvergent paths (x AND NOT x) give X where the real value is fixed. A toggle latch fed by an X edge stays X for the rest of the step. So the proven share will be below 2/3, possibly far below. On a bus that carries an instruction word in one context and data in another, the bus is static in the first and dynamic in the second, which is what is wanted.
- **Cheapest experiment.** An estimator on the reference engine: per context, take the bits that differ among the observed start states as X, run one ternary simulation of the step from one recorded start, and count the flips that stay binary. Compare with the 62% to 70% observed share. A day; no engine change. This is the single most informative experiment in this report.
- **Tags.** Source support: verified (Bryant's soundness statement, Facile's binding-time labels); the application is our own. Fit: plausible as an estimator; what it enables is a long shot.

### A2. Per-context residual simulation ("Facile on a netlist")

- **What it is.** For each context, keep a record built once from A1: the static flips with their tick offsets, the static part of the end state, and the list of dynamic gates. At run time, at the start of a step in a known context, check the static key bits (one masked compare of the packed state). Then simulate only the dynamic gates. Static flips are not evaluated or queued. Their effect on dynamic gates is applied as a precomputed per-tick list of count changes ("at offset 7: count of gate 812 +1"). At the step end, the static part of the end state is applied as one precomputed XOR diff. If a dynamic gate's outcome reaches something the record says is static (the proof was per start state class, so this cannot happen if A1's proof holds), there is nothing to check; the guard is the static key at the start.
- **Where it comes from.** Facile's fast simulator: it replays recorded "action numbers" for the static work and runs only the dynamic basic blocks, with "dynamic result tests" where a dynamic value picks the next action. On a miss, Facile's slow simulator re-runs only the static part up to the miss, without dynamic side effects, then continues normally. Facile reports an average 8.3 times gain from memoization on its out-of-order simulator, and FastSim (the hand-written version) 4.9 to 11.9 times. Applying it per context to a netlist is our own.
- **What is reported.** Facile/FastSim figures above, on processor microarchitecture simulators, not netlists.
- **Why it might help here.** Step 8 closed "replay the control, simulate the data" because a *global* control/data split holds only one flip in nine. A per-context split does not have that problem: a gate is static in one context and dynamic in another, and the record is per context. With only a few hundred contexts, the records are small enough to keep. The step-8 ceiling (two thirds of the flips free) is about 2.3 times on the simulated part.
- **Why it might not.** Three costs. (1) Count changes from static gates into dynamic gates still have to be applied every tick; if dynamic gates read many static signals, the saving shrinks (step 8 counted 0.16 such updates per tick with a global split; per context it is unknown). (2) Applying the static end-state diff costs per differing bit (measurement N decides). (3) The proof is per class of start states; a context whose static bits vary needs several records. It is also a lot of machinery, and attempt 12 showed machinery can eat the gain.
- **Cheapest experiment.** Extend the A1 estimator: per context, count (a) proven-static flips, (b) count updates from static to dynamic gates per tick, (c) net static end-state diff. Model the cost as (b) + (c) + dynamic flips, against today's flips. Only build if the model gives more than about 1.3.
- **Tags.** Source support: verified (Facile mechanism and figures, read in the paper); the transfer is our own. Fit: long shot, but the only one aimed squarely at the 2/3.

### A3. Per-context "don't-care" masks on the replay key

- **What it is.** Replay keys on the whole state. Some state bits are overwritten in the step before anything reads them (a temporary register, a bus latch that is reloaded). For such a step the bit's value cannot matter. Run the step once in ternary simulation with the candidate bit(s) set to X. If the end state, the event tick and the event address and word come out binary, the step is proven independent of those bits *given the other bits*. Store the replay entry with those bits zeroed in its key and a per-entry mask. At lookup, zero the same bits and look up as today: still one exact hash lookup, no candidate scanning.
- **Where it comes from.** Our own. It is the sound, exact-hash form of open idea 10 (read-set replay). The soundness argument is Bryant's covering property. Masking a key before hashing is the "don't-care" idea from state hashing in model checking (state abstraction), but no source describes this exact use.
- **What is reported.** None.
- **Why it might help here.** Step 1 found that matching on read sets instead of whole states would raise replay from 36% to 55% (day 1 part 2) and 44% to 55% (day 4 part 2). The obstacle was the lookup: a read set has about 820 signals and finding a match needs comparisons against candidates. Masks keep the lookup an ordinary hash lookup. The context (and so the mask to try) can be picked from what is known at the step start: the address of the event and the event kind. A context has only a few masks.
- **Why it might not.** (1) Only the bits that are dead in the step qualify, a subset of what read sets leave out; the gain is below the 55% ceiling, perhaps far below. (2) The proof run is a ternary simulation, about twice the cost of a simulation, and it would run on every newly learnt step, which is every simulated step. Done on the main thread this doubles the cost of the 88%. It only pays if done on another core (see E2) or only for contexts that miss often.
- **Cheapest experiment.** In `readset_estimate.rs`, split the bits a step does not read into "overwritten before read" (dead) and "never touched". Recompute the share of steps seen before when dead bits are masked. If it is not well above today's replay share, drop the idea.
- **Tags.** Source support: our own (soundness from Bryant, verified). Fit: plausible as an estimate; build only with E2.

### A4. Read-set replay, revisited with a cheaper lookup and a cheaper apply

- **What it is.** Keep, per context, the last few learnt steps as (start bits, read mask, start-to-end XOR diff, event). On a miss, test each candidate with a masked compare over the packed state: `((current XOR start) AND mask) == 0` over 33 to 50 words, about 10 ns per candidate. On a match, the step is identical (same reads, deterministic circuit), so the end state is current XOR diff. Apply it with the existing diff-based `restore`.
- **Where it comes from.** Trace-level reuse (González, Tubella, Molina, ICPP 1999) and dynamic instruction reuse (Sodani and Sohi, ISCA 1997) reuse a computation when its live-in values match, which is the read-set rule. The per-context candidate list and the masked compare are our own.
- **What is reported.** Trace-level reuse: speedups "around 3" for a 256-entry window in a processor model (claimed by an abstract, not read). Nothing for simulators.
- **Why it might help here.** This is idea 10, which step 1 bounded at 1.35 and 1.2 on the two weak programs. The new angle is the cost model. Step 1 assumed a lookup against several candidates of ~820 signals is expensive; with packed bitmasks it is tens of nanoseconds, against about 3 microseconds for a simulated step. And the apply cost is the *net* diff (measurement N), not the 320 flips.
- **Why it might not.** Learning the read set costs on every simulated step: each evaluation must mark the inputs it read that were not yet written in the step. With about 2.5 inputs per evaluation and 6 to 8 evaluations per tick, that is perhaps 10% to 20% on the 88%, paid on all boards, for a gain on two programs. Steps 1 and 8 already said "leave it". It has effectively been tried as an estimate; only the cost model is new.
- **Cheapest experiment.** Measurement N, plus counting how many candidates per context would have to be scanned to reach the step-1 hit rate. If N is small and a handful of candidates suffice, write the learning cost into the model and decide.
- **Tags.** Source support: claimed (reuse papers read only as abstracts). Fit: long shot (the bound is 1.35 on the weakest programs).

### A5. Step summaries as copy and affine maps over the data bits

- **What it is.** Simulate a step once per context with a small symbolic value domain: each signal is 0, 1, "start bit i", "NOT start bit i", or "too complex". AND with a constant 0 gives 0, AND with 1 passes the symbol through, XOR of two symbols can be kept as a two-term XOR, anything else becomes "too complex". If the end state and the event come out without "too complex", the step is a copy/affine map: end bit j = start bit i, or its inverse, or a constant. Such a step can be replayed for *any* data: compute the end state by a bit permutation of the start state and apply it with the diff-based restore.
- **Where it comes from.** Our own. It is the "carry the data through as a function of what it read" that step 1 named as the remaining direction with no estimate. Symbolic simulation of datapaths is standard in verification (Springer reference on symbolic simulation; a DATE 2026 paper shows BDDs for affine spaces stay linear in size), but I found no simulator that uses an affine or copy domain to replay steps.
- **What is reported.** None for simulation.
- **Why it might help here.** Many CPU steps move data without computing on it: fetch an instruction word, move a register, load a word into a register, write a register to memory. Such steps are exactly where whole-state replay misses (the data differs) and where the control is identical. A move step would become a key check plus a bit permutation.
- **Why it might not.** Additions, comparisons and anything with a carry chain are not affine, so ALU steps stay simulated. The bookkeeping of applying a permutation is the net changed bits (measurement N) times the fan-out count updates, plus building the result. It also needs the same key-and-guard machinery as A2.
- **Cheapest experiment.** In the A1 estimator, add the copy/affine domain and count the share of simulated steps (and simulated ticks) whose whole effect is copy/affine. Also count those steps' net changed bits.
- **Tags.** Source support: our own. Fit: long shot.

### A6. Simulate only the divergence from a per-context reference run

- **What it is.** Keep for each context one reference execution (its flips with tick offsets). Represent the current step as "the reference, plus a set of signals whose value differs from the reference at this tick". Simulate only events in that divergence set. A gate is evaluated only when one of its inputs is in the divergence set; its other input values are read from the reference. Signals leave the set when they converge again.
- **Where it comes from.** Concurrent fault simulation (Ulrich and Baker, 1970s): the good machine is simulated once, and each faulty machine is carried only as the gates where it differs from the good machine, with divergence and convergence of these lists (KIT lecture notes on fault simulation; a DTIC report called it "most promising" of the three methods in the 1980s). The use for a single run against a recorded reference is our own.
- **What is reported.** For fault simulation: faster than deductive and parallel fault simulation (lecture notes, no figure read).
- **Why it might help here.** It is the most direct reading of "two thirds of the flips are identical in every execution of the context": only the third that differs is work.
- **Why it might not.** In fault simulation the good machine is simulated anyway and its cost is shared by thousands of faulty machines. Here there is one machine, so "read the reference value of an input at this tick" must be a lookup into a per-context table of when each signal flips. That lookup per input of every divergent gate is the bookkeeping that sank attempt 12 and step 8. And at the step end the real state still has to be materialised. This is A2 without the proof, with more lookups.
- **Cheapest experiment.** From the step-8 data: per simulated step, the number of divergent flips and the number of reference-value lookups they would need (inputs of the gates they wake). If lookups times about 3 ns plus divergent flips times 13 ns is not well under all flips times 13 ns, drop it.
- **Tags.** Source support: claimed (lecture notes and a DTIC report, read only as summaries). Fit: probably not, kept because it is the textbook form of the idea.

### A7. Look for reconvergence inside a step, not only at its end

- **What it is.** Replay looks up the state only at memory events. If a step overwrites the bits that made it new (a register reloaded from memory), the state may become one seen before *in the middle* of the step, and the rest of the step need not be simulated. Add lookup points inside simulated steps, for example at quiet ticks (no queued gates) or every k-th simulated tick, as extra replay nodes with edges to the next event.
- **Where it comes from.** Our own. It is like Hashlife's "find a known block and jump", applied to time only.
- **What is reported.** None.
- **Why it might help here.** The packed state is already maintained at every flip (attempt 19), so hashing at a checkpoint costs about the 33 to 50 words of the state. A step that converges at tick 20 of 41 simulated ticks saves half of it.
- **Why it might not.** Data differences usually persist until the end of the step (registers keep their values), and a difference that disappears mid-step would also make the step end a known state, so only the simulated tail after reconvergence is saved. Every lookup that misses costs about 20 to 50 ns.
- **Cheapest experiment.** In `replay_estimate.rs` or the profiling build: at every quiet tick of a simulated step, test whether the state is in a set of states seen at quiet ticks before; report the share of simulated ticks that come after such a hit.
- **Tags.** Source support: our own. Fit: long shot, cheap to measure.

---

## Group B. Compiling and specialising

### B1. Per-context traces with guards and side exits (tracing JIT for the event loop)

- **What it is.** For a hot context, record the step as a linear trace: the flips in order, the count updates, and a guard wherever a dynamic gate decided something. Compile the trace to machine code. Run it on the next execution of the context; a failing guard exits to the interpreter at that point (the state is consistent because the trace applies flips as it goes). Chain traces of consecutive contexts.
- **Where it comes from.** Meta-tracing (Bolz et al., "Tracing the meta-level", ICOOOLPS 2009: tracing an interpreter needs hints, otherwise "very limited or no speedup"); RPython's guard handling (Bolz et al., 2012); QEMU's translation-block chaining (TCG developer docs, read). Applying it to event streams of a netlist is our own; this is idea B5 of the v2.1 file with the guard model added.
- **What is reported.** PyPy: large speedups on interpreters (not read in detail). QEMU: no figure in the docs.
- **Why it might help here.** It removes the queue, the dirty list, the `match` on gate kind and the wake test for every flip on the trace; all decisions that are static become straight-line code with constant indices. The few hundred contexts per program are a small number of traces.
- **Why it might not.** Code size. A step is about 300 flips times about 3 count updates; a few hundred contexts give roughly 100,000 to 300,000 updates of machine code, several megabytes. ESSENT and the Dedup paper (ASPLOS 2024) both found that simulator code that outgrows the caches is the bottleneck; here the data fits the first-level cache and the code would not. Writing a JIT in the engine is also a large project, and the data-dependent third still needs guards everywhere.
- **Cheapest experiment.** Count, per context, the trace length and the number of guards a trace would need (dynamic decisions per step). Multiply by an assumed 8 to 12 bytes per update to get the code size. If the hot contexts that cover 80% of simulated ticks do not fit in about 1 MB, drop it.
- **Tags.** Source support: claimed (PyPy papers as abstracts) and verified (QEMU chaining docs). Fit: long shot.

### B2. Copy-and-patch as the code generator for B1 or B3

- **What it is.** Build a library of small machine-code templates ("stencils") at build time, for example "add +1 to the count at offset K and compare with T". At run time, compile by copying stencils and patching the constants. No compiler in the engine.
- **Where it comes from.** Xu and Kjolstad, OOPSLA 2021 (abstract read). Used by the CPython 3.13 JIT.
- **What is reported.** Compile time far below LLVM -O0 with code quality above it (abstract; figures not read). An R JIT reports 980 bytecode instructions compiled per millisecond.
- **Why it might help here.** It makes B1 or B3 a smaller project than embedding LLVM or Cranelift.
- **Why it might not.** It is only an enabler; it does not change the code-size problem of B1.
- **Cheapest experiment.** None on its own; decide after B1 or B3's estimate.
- **Tags.** Source support: claimed. Fit: plausible as an enabler only.

### B3. Compiled event-driven code per entity

- **What it is.** Generate one small routine per simulated entity that applies its flip: the fan-out is hard-coded (constant addresses of the readers' counts, constant thresholds, the wake rule of each reader's kind), so there is no walk over the `writes`, `direct` and `readers` lists and no `match`. The event loop calls the routine of each flipping entity.
- **Where it comes from.** Lewis, "Hierarchical compiled event-driven logic simulation", ICCAD 1989 and IEEE TCAD 1991 (abstract: "up to a factor of 10 over conventional event-driven simulation"; a fan-out procedure and a simulation procedure per unique module). Weise, Seligman and Au (1991) produced compiled simulators by partial evaluation of an interpreted simulator, the Futamura route.
- **What is reported.** Lewis: up to 10 times over conventional event-driven simulation, on hardware of 1991 (claimed, abstract only). Weise: no figure in the abstract; it says the compiled form of one algorithm (BACKSIM) was not worth it.
- **Why it might help here.** `propagate` is 60% of the time and is a nested walk over three lists. About 2,100 to 3,100 entities with a fan-out of about 3 give a small amount of code (perhaps 50 to 150 KB), which may fit the instruction cache, unlike B1.
- **Why it might not.** Lewis compared with interpreters of 1991; v2's interpreted loop is already flat, branch-light, profile-optimised and L1-resident. An indirect call per flip may cost as much as the list walk it replaces (an indirect branch to one of 2,000 targets is hard to predict). Attempt 21 showed layout gives nothing; this is about instructions, which is where the time is, but the gain is unknown.
- **Cheapest experiment.** Hand-write (in a throwaway branch) the generated code for the entities that make the top 50% of flips on one board, called through a function-pointer table, and time `propagate` alone against today's on a recorded flip stream. A day.
- **Tags.** Source support: claimed (abstracts). Fit: plausible.

---

## Group C. The per-flip kernel

### C1. Decide the flip when the count changes, and cancel pairs

- **What it is.** Today a woken gate goes into `dirty`, and the next tick evaluates it again from its count and threshold. Instead, decide in `feed` whether the gate will flip, by the crossing of the threshold, and toggle a "will flip" mark. Two crossings in the same tick cancel. The tick then applies the marked gates without an evaluation pass.
- **Where it comes from.** Maurer, "The Inversion Algorithm for Digital Simulation", ICCAD 1994 (read): "The Inversion Algorithm will not schedule a gate for simulation unless its output will change value"; for XOR gates already scheduled, "the two simulations will cancel each other"; counting is done "during the event processing phase, rather than the gate simulation phase", so a gate's simulation is only an inversion.
- **What is reported.** "Performance rivals or exceeds that of Levelized Compiled code simulation, even at activity rates of 50% or more" (zero-delay, 1994).
- **Why it might help here.** v2 already has counts, thresholds and "do not wake a gate that cannot change" (attempts 4 and 5); this is the next step of the same idea. It removes one pass over the woken gates (a second cell access and a `match`) per evaluation. With 6 to 8 evaluations per tick and the "rest of the tick" at 28%, a few percent is plausible.
- **Why it might not.** With unit delay and edge-counting latches, the exact semantics (a latch toggles once per rising input, two together cancel; a VMem pin's input is lost while locked) must be kept, and the pin lock is decided at the tick, not at the feed. Gates fed twice in one tick (up then down) already end up not flipping today, so the saving is only the evaluation pass. Some of this is effectively done.
- **Cheapest experiment.** Count, in the profiling build, how many evaluations per tick end with no flip (step 0 says 6 to 7 evaluations and 6 to 7 flips, so nearly none). If nearly every woken gate flips, the gain is only the second access; try it as an attempt of a few hours.
- **Tags.** Source support: verified. Fit: plausible, small.

### C2. Fan-out lists split by reader kind, or a handler per fan-out edge

- **What it is.** Today `feed` does a `match` on the reader's kind for every count update. Split each entity's reader list into sub-lists by wake rule (threshold gates, XOR-like gates, latches and pins), so each inner loop has one rule and no branch on kind. Maurer's version stores per fan-out branch a pointer to its handler, and AND/OR inputs alternate between an increment and a decrement handler so the "+1 or -1" choice disappears.
- **Where it comes from.** Maurer 1994 (read): per-branch "shadows" with a "subroutine" field; INCREMENT/DECREMENT pairs; NOT and BUFFER eliminated by scheduling their fan-out directly (zero-delay only).
- **What is reported.** As for C1.
- **Why it might help here.** Threshold gates (AND, OR and their inversions) are most of the evaluations (AND alone 61% to 80% in v1's profile), so the threshold loop would carry almost all the work with no kind branch.
- **Why it might not.** With PGO and a predictable mix, the branch on kind may already be almost free. The NOT/BUFFER elimination does not carry over: under unit delay a buffer adds a tick (see C4 for the version that does).
- **Cheapest experiment.** Measure branch mispredictions per flip first (C5). If the `match` in `feed` is not a visible source of mispredictions, skip.
- **Tags.** Source support: verified. Fit: plausible, small.

### C3. Time-bit-parallel waveforms for the active gates

- **What it is.** Represent the value of a signal over the next 64 ticks as one 64-bit word, bit t = value at tick t. A gate with unit delay gives `out = (op(in1, in2, ...) << 1) | out_now`, one word operation for 64 ticks. A toggle latch is a prefix-XOR over its rising edges (`rise = in & ~(in << 1)`, then a log-step prefix XOR), also a few word operations. Process only the gates whose waveform changes, in event-driven order; with feedback loops, iterate until the waveforms stop changing (each pass fixes at least one more tick, because every gate delays by one).
- **Where it comes from.** Maurer's "parallel technique" for unit-delay compiled simulation (IEEE TCAD 1992) gives each net an n-bit field and uses bit-parallel operations, with "shift elimination" and "bit-field trimming"; it reports about 10 times over interpreted event-driven simulation on ISCAS-85. Lee and Maurer, "Bit-parallel multidelay simulation" (1996), uses "wide bit-fields containing net-values for several different times". I could not read either paper; the "one bit per tick" reading is inferred from "shift elimination" and the 1996 abstract. Doing it event-driven, with loops, is our own.
- **What is reported.** About 10 times over interpreted event-driven simulation, for acyclic circuits within one clock cycle (claimed).
- **Why it might help here.** The v2.1 file closed "word-wide simulation of bit slices" because it evaluates the whole circuit every tick. This is different: the parallelism is across *time*, not across gates, and only the gates that are active in the window are touched. A step's 41 busy ticks fit in one word. If a typical active gate flips several times in a window, one word operation replaces several flips.
- **Why it might not.** The CPU is full of feedback (latches feed logic that feeds latches), and every loop around a feedback path within the window needs another pass. If signals go around loops many times per window, passes multiply and the gain disappears. VMem (the lock, the load arriving one tick later) breaks the window and must be handled at its exact tick. Unknown and possibly large engineering.
- **Cheapest experiment.** From a recorded flip stream of simulated steps: (a) flips per distinct flipping entity per 64-tick window (if it is near 1, there is nothing to gain); (b) the number of passes a waveform iteration would need, which is the longest chain of "A flips, then a reader of A flips later in the window" that closes a loop.
- **Tags.** Source support: claimed. Fit: long shot.

### C4. Collapse chains of single-input gates into delay lines

- **What it is.** A chain of k buffers or NOT gates, each with one reader, is a delay of k ticks with an inversion parity. Replace it by one delay line: when its input flips at tick t, schedule its output flip at t + k in a small timing wheel. All k intermediate flips vanish from the simulation, but they are still state: they are kept as "in flight" in the wheel and expanded into the packed state only when replay needs a key (or kept out of the key if the wheel contents are hashed instead).
- **Where it comes from.** Timing wheels: Varghese and Lauck, SOSP 1987 (summary read). Redpiler (MCHPRS, the Minecraft redstone compiler, docs read) models repeaters as single nodes with a delay and a tick scheduler of queues per delay and priority. Maurer removes NOT and BUFFER gates in zero-delay simulation. The unit-delay version with the state kept exact is our own. Attempt 10 merged single-writer *traces*; it did not touch gate chains.
- **What is reported.** None for this use. Redpiler's docs give no figures.
- **Why it might help here.** In pixel-art circuits long wires are often drawn with buffers to carry signals or create timing; NOT and Buffer were 2% each of v1's evaluations, but their share of flips in the simulated part is not known.
- **Why it might not.** The packed state must stay exact at every event, so the in-flight edges must be part of the replay key. Pulses (several edges in flight) need a queue per line. If chains are short (k = 1 or 2), there is no gain.
- **Cheapest experiment.** A `netlist_stats`-style count: chains of single-input, single-reader gates by length, and the share of all flips (from `profile`) made by gates inside such chains. Minutes.
- **Tags.** Source support: verified for the parts (Redpiler docs, timing-wheel summary); combination our own. Fit: long shot, cheap to check.

### C5. Measure branch mispredictions and instructions per flip before more kernel work

- **What it is.** Use the CPU's performance counters (on macOS: Instruments' CPU Counters template or `xctrace`) on a run with replay off to get instructions, cycles and branch mispredictions per flip. That tells whether `propagate` is bound by instruction count, by mispredictions, or by dependency chains (counts updated through memory).
- **Where it comes from.** Standard practice; ESSENT's branch-hint gain and GSIM's statement that 82% of executed branches in their simulator check active bits point at branches as the cost of sparse simulation.
- **What is reported.** GSIM: "82.26% of all executed branch instructions are dedicated to examining the active bits" (read).
- **Why it might help here.** Attempt 21 proved the tick is not memory-bound. What is left is instructions and branches, but nobody has measured which. Ideas C1, C2 and B3 each attack a different one.
- **Why it might not.** It gains nothing by itself.
- **Cheapest experiment.** It is the experiment. Hours.
- **Tags.** Source support: verified (GSIM figure). Fit: plausible (as a measurement).

### C6. Several activity flags tested in one compare (GSIM's fast path)

- **What it is.** GSIM keeps one active bit per node and per supernode, and tests eight of them at once (`if (active[7:0] != 0)`) on the assumption that most are clear.
- **Where it comes from.** GSIM, arXiv 2508.02236 (read).
- **What is reported.** GSIM: 7.34 times over Verilator booting Linux on XiangShan, 19.94 times on CoreMark on Rocket; supernodes of 20 to 50 nodes; activity factor 4.61%.
- **Why it might help here.** It does not, as far as I can see: v2 never scans flags; it keeps a list of exactly the woken gates.
- **Why it might not.** GSIM evaluates in a fixed order every cycle and skips inactive parts; at 0.2% activity a list is better than any scan. This is the per-partition idea that is already closed.
- **Cheapest experiment.** None.
- **Tags.** Source support: verified. Fit: probably not.

### C7. Lazy evaluation of logic that is shadowed by a closed gate

- **What it is.** When every reader of a signal is an AND (or OR) whose other input currently forces its output, a change of the signal cannot go anywhere. Do not propagate it now; mark the region stale and bring it up to date when the forcing input releases or when the state must be hashed. Transient flips in a stale region (a bus that changes twice while its enable is closed) are never simulated.
- **Where it comes from.** ESSENT's slides list skipping "unselected mux inputs"; Maurer (1994) names "useless simulations of the second kind" (a change "absorbed before reaching a monitored net") as "significantly more difficult" to remove. Weise et al. (1991) found that compiled BACKSIM, a demand-driven method, cost more than it saved.
- **What is reported.** ESSENT: no per-technique figure. Weise: negative for BACKSIM.
- **Why it might help here.** Count-based waking already stops at a closed gate (the gate is not queued), so this would only save flips *inside* the shadowed region, of signals that are themselves state.
- **Why it might not.** The packed state must be exact at every event, and a stale region must then be brought up to date with its exact values at that tick, which can mean re-simulating its last ticks. Toggle latches inside the region count edges, so transients cannot simply be dropped. The one negative source points the same way.
- **Cheapest experiment.** Count flips of signals all of whose readers were forced at that tick. If small, drop.
- **Tags.** Source support: claimed (slides, abstracts). Fit: probably not.

---

## Group D. The replay table and state hashing

### D1. Zobrist-style hashing with one random key per signal

- **What it is.** Give each signal a random 64-bit key and keep the state hash as the XOR of the keys of the signals that are on, updated with one XOR per flip, as chess programs do.
- **Where it comes from.** Zobrist hashing (Wikipedia and the Chess Programming Wiki, summaries read).
- **What is reported.** Standard in game engines; no figure relevant here.
- **Why it might help here.** It would make the hash at each event free.
- **Why it might not.** Attempt 19 already keeps the packed state up to date at every flip, and lookups were 2% of the time after it. A hit must still compare the full stored state (the project's rule), so only the hash itself is saved, a few nanoseconds per event. It also adds one XOR and a key load to every flip, which is where the time is. Effectively already done.
- **Cheapest experiment.** None needed: the hashing share in the profile is the upper bound.
- **Tags.** Source support: verified. Fit: probably not.

### D2. A replacement policy instead of flushing the whole table

- **What it is.** At the memory limit, attempt 22 empties the table. Transposition tables in chess programs instead replace entries by a policy (keep the deeper search; two entries per bucket, one always replaced). Here: keep nodes that were reused or whose edges cover many ticks; drop the rest.
- **Where it comes from.** Chess Programming Wiki on transposition tables (summary read). FastSim and Facile flush when full (Facile: "cache size can be reduced by a factor of ten, with little impact", read).
- **What is reported.** Facile's flush result above supports the existing choice.
- **Why it might help here.** Only under a memory limit.
- **Why it might not.** Step 5 found states are reused soon after they are learnt and a 32 MiB table is nearly as good as an unlimited one. Facile says the same. A policy adds per-node counters on the hot path.
- **Cheapest experiment.** Run day 4 part 2 at 8, 16 and 32 MiB and see whether anything is left to win.
- **Tags.** Source support: verified (Facile). Fit: probably not.

### D3. Compressing stored states by sharing sub-vectors (tree or collapse compression)

- **What it is.** Split each stored state into fixed chunks, intern each chunk in its own table, and store the state as a short vector of chunk indices, recursively as a tree. Similar states share most chunks.
- **Where it comes from.** SPIN's COLLAPSE mode (Holzmann); tree compression in LTSmin (Laarman, van de Pol, Weber, 2011; arXiv abstract read): about 4 to 5 times better compression than collapse at the median; one model at 188 MB against 1.5 GB (collapse) and 3 GB (plain table).
- **What is reported.** Above (claimed from the abstract and a summary).
- **Why it might help here.** It would shrink the table and the `--replay-cache` file several times, and partial matches become visible (states that share most chunks).
- **Why it might not.** Speed is not limited by table size (step 5). Interning each chunk on every store adds work on the 12% replay side. Comparing a hit becomes a tree walk.
- **Cheapest experiment.** Count distinct 64-bit words per word position across stored states; the ratio gives the compression factor. Only if memory ever becomes the limit.
- **Tags.** Source support: claimed. Fit: probably not for speed.

---

## Group E. Other cores and time-parallel ideas

### E1. Speculating on steps ahead, checked with read sets (thread-level speculation over steps)

- **What it is.** When the main thread misses at state S, a helper core guesses the state S1 at the next event: the arrival recorded from S for a *different* word, or the arrival of the last step in the same context. It simulates on from S1 and records for each speculative step its read mask and its start-to-end diff. When the main thread reaches the real S1, it checks with one masked compare (a few dozen word operations) whether the real state agrees with the guess on the speculative step's read set. If it does, it takes the result with a diff-based restore and goes on.
- **Where it comes from.** Time-parallel simulation and its "state matching problem" (several Winter Simulation Conference papers, summaries read; fix-up computations attributed to Heidelberger and Stone 1990). Thread-level speculation in processors (no source read in this pass). The read-set check and the use of replay arrivals as guesses are our own.
- **What is reported.** None for this setting. Exact time-parallel methods for deterministic systems need the guess to be right; the papers that gain use approximate matching (not allowed here) or special structure (queues, via parallel prefix).
- **Why it might help here.** It is the only way I found to use other cores on one run without guessing memory words (which step 5 argued against). The check is cheap because the read mask is a bitmask over the packed state. The helper does all the costly read-set tracking.
- **Why it might not.** The guess must agree on everything the next step reads; step 1 found read-set matching adds only 10 to 20 points of hits on the weak programs, and that is with any earlier step, not a guessed one. A wrong guess wastes only helper time, but a right one saves a step only if the helper got there first. Synchronisation per step (about 3 microseconds of work) is affordable; per tick it would not be.
- **Cheapest experiment.** Single-threaded estimator: at each simulated step, take the guess (last arrival in the same context), compute the next step's read set from the real run, and count how often the guess agrees on it. That share is the upper bound of steps a helper could save.
- **Tags.** Source support: claimed (time-parallel summaries); mechanism our own. Fit: long shot.

### E2. A helper core that certifies and learns, never on the critical path

- **What it is.** Give the costly proof work of A1 and A3 (ternary simulation of a step to prove a mask or a static set) to another core. The main thread sends (start state, step's event, context) to a queue and goes on. The helper proves and inserts masked entries into the replay table. The main thread only reads the table.
- **Where it comes from.** Our own. The pattern is that of SuperPin (Wallace and Hazelwood, CGO 2007) and PiPA (CGO 2008): a fast main run forks off slow work for slices (abstracts read).
- **What is reported.** SuperPin: "often order-of-magnitude" gains for instrumentation (claimed). Not comparable.
- **Why it might help here.** It removes the objection to A3 (the proof doubles the cost of every learnt step). The table is already shared state; one writer and one reader is the simple case.
- **Why it might not.** It is only worth anything if A3's estimate is good. A table written by another thread needs care (the table is a plain hash today).
- **Cheapest experiment.** A3's estimator decides.
- **Tags.** Source support: claimed for the pattern. Fit: plausible only together with A3.

### E3. Moving the state interning to another core

- **What it is.** At each event of a simulated step, the main thread hands the packed state (33 to 50 words) to a helper that hashes, compares and stores it.
- **Where it comes from.** Our own; the producer/consumer pattern as in PiPA.
- **What is reported.** None.
- **Why it might help here.** It takes the store work off the main thread.
- **Why it might not.** The main thread needs to know at once whether the next step is a hit, so only the storing of new states can move, and replay plus all its bookkeeping is 12%. Copying the state costs about as much as hashing it.
- **Cheapest experiment.** The profile already gives the bound (the "storing new states" share, about 3%).
- **Tags.** Source support: our own. Fit: probably not.

### E4. Parallel in time by guessing segment start states (parareal-like)

- **What it is.** Split the run into segments, start each from a guessed state, then fix up.
- **Where it comes from.** Time-parallel simulation literature (summaries read).
- **What is reported.** Gains only with approximate state matching or for models with special structure.
- **Why it might help here.** It would not; kept so it is not searched again.
- **Why it might not.** For a deterministic exact system, a wrong guess is worthless, and there is no cheap coarse model to guess from. E1 is the only form that seems to keep exactness.
- **Tags.** Source support: claimed. Fit: probably not.

---

## Group F. Abstraction and memoization over bigger units

### F1. Recognising words and registers in the netlist

- **What it is.** Find groups of latches that act as one register (same control signals, parallel data paths) and groups of signals that form a bus, from the netlist alone.
- **Where it comes from.** WordRev (Li et al., HOST 2013); Tashjian et al. (DAC 2015: find words from a few shared control signals); Yu et al. (ISCAS 2016: word-level abstraction by algebraic rewriting) (abstracts read).
- **What is reported.** Reverse-engineering accuracy; nothing about simulation speed.
- **Why it might help here.** It is an enabler: A5 (copy maps) and F2 (loop acceleration) are much easier when the data bits come in words. It also gives a principled, board-independent definition of "data" for A2.
- **Why it might not.** It is a large piece of analysis with no direct gain, and step 8 showed the same gates act as control and data in different places.
- **Cheapest experiment.** Group latches by their identical sets of non-data inputs (enables, clocks) and see whether 8- or 32-wide groups appear on all three boards.
- **Tags.** Source support: claimed. Fit: long shot.

### F2. Accelerating counter loops with a closed form

- **What it is.** When a sequence of steps repeats with the same control and only a counter-like value changing by a constant, compute the state after n iterations directly and jump.
- **Where it comes from.** Loop acceleration in program analysis (LoAT, "A calculus for modular loop acceleration", arXiv; abstracts read). Busy Beaver simulators accelerate repeating configurations in a similar spirit (only secondary sources found).
- **What is reported.** Verification tools only; no simulator figure.
- **Why it might help here.** AoC programs loop over input with counters; replay misses every iteration because the counter differs.
- **Why it might not.** The loop's exit and every memory event inside it must still happen at their exact ticks, and memory reads inside the loop depend on the counter. It needs A5's symbolic machinery plus word recognition plus a proof that the pattern holds. Very large for an uncertain gain.
- **Cheapest experiment.** Count runs of consecutive steps with the same context sequence and states that differ only in a few bits.
- **Tags.** Source support: claimed. Fit: long shot.

### F3. Hashlife's doubling, over steps

- **What it is.** Memoize not only "step from S" but "two steps from S", "four steps from S", and so on, to jump further per lookup.
- **Where it comes from.** Hashlife (Gosper 1984; johnhw's annotated implementation, read): the successor of a 2^k block is memoized for 2^(k-2) generations; "It is not efficient for chaotic patterns (it will rapidly run out of memory and grind to a halt)" and "very fast for periodic patterns run for long stretches".
- **What is reported.** Astronomical speedups on regular patterns; none on chaotic ones.
- **Why it might help here.** It would not: attempt 9 (chaining replays) showed the return to the run loop is not where the time is (0.98). Doubling saves lookups, and replayed ticks already cost about 1% of simulated ones.
- **Why it might not.** See above; also a doubled step needs every memory word in it to match.
- **Tags.** Source support: verified. Fit: probably not.

### F4. Hashlife's space-time blocks on a netlist

- **What it is.** Memoize the future of a block of the circuit over a window, keyed on the block plus its neighbourhood, relying on a bounded propagation speed.
- **Where it comes from.** Hashlife (as above). I found no source that generalises it to irregular graphs; the material found is grid-only, and one source says it does not apply directly to larger neighbourhoods.
- **What is reported.** Nothing for graphs.
- **Why it might help here.** It would not on present evidence.
- **Why it might not.** On a netlist the "neighbourhood" of a block over k ticks is its k-deep input cone, which grows fast and changes often. Attempt 12 (parts with their own tables) is exactly this with k = 1 and lost at every size. The Wireworld/Golly question (Hashlife on a circuit CA) gave no checked answer; the likely reason it helps little is the same constant signal traffic.
- **Tags.** Source support: verified (grid-only, chaotic weakness). Fit: probably not.

### F5. A whole step as a decision diagram walk

- **What it is.** Build a decision diagram for the step's outcome as a function of the state and evaluate it by table lookups.
- **Where it comes from.** McGeer et al., "Fast discrete function evaluation using decision diagrams", ICCAD 1995; Ashar and Malik, ICCAD 1995 (abstracts read).
- **What is reported.** "Orders of magnitude" in theory for cycle-based simulation, with memory bandwidth dominating on large designs.
- **Why it might help here.** It would not; it is the "lookup tables for cones" idea already closed.
- **Why it might not.** A 41-tick step over 2,000 signals has no small diagram.
- **Tags.** Source support: claimed. Fit: probably not.

---

## The five I would look at first

**1. The ternary-simulation estimator (A1), with measurement N alongside.** Every promising idea about the per-context regularity (A2, A3, A5) depends on one unknown: how much of the observed two thirds can be *proven* static per context. Ternary simulation answers it soundly and without any board knowledge, using the reference engine and the step-8 context grouping that already exist. Measurement N (net changed bits per step against flips) is a few extra lines and decides whether "apply the result instead of simulating it" can ever be cheap. A day of work settles whether the whole of group A is alive.

**2. Per-context don't-care masks (A3), estimated with the existing read-set tool.** It is the only form of "replay more" I found that keeps the lookup a single exact hash and is sound by construction. Its gain is bounded above by step 1's read-set figures, so the estimate is cheap: split unread bits into "dead in this step" and "untouched" and recount hits. If the hit gain is real, the proof cost can go to a helper core (E2), which is the cleanest use of other cores I found.

**3. Decide the flip in `feed` and cancel pairs (C1), after measuring branches and instructions per flip (C5).** It is the published next step of the counting method v2 already uses (Maurer's Inversion Algorithm, read), it is a few hours of work, and it attacks the part of the tick outside `propagate`. Measuring mispredictions and instructions per flip first says whether C1, C2 or B3 is aimed at the actual cost; no one has measured that since attempt 21 showed memory is not it.

**4. Per-entity compiled fan-out (B3), as a hand-written micro-benchmark.** `propagate` is 60% of the time, and it is a walk over three lists with a `match` per reader. A per-entity routine with constant addresses is the oldest published answer to exactly that (Lewis 1989/1991), and unlike per-context traces (B1) its code is small enough to stay in the instruction cache. The risk is an unpredictable indirect call per flip; a one-day micro-benchmark on a recorded flip stream answers it.

**5. Delay-line chains (C4) as a netlist count.** It costs minutes to count chains of single-input gates and their share of the flips. If pixel-art wiring makes long buffer chains common in the simulated part, collapsing them removes flips outright, which is the kind of gain attempt 6 (dropping dead logic) gave. If chains are short, it is closed for good.

---

## Looked for and found nothing useful

- **Hashlife on irregular graphs or netlists.** Every source found is about square grids with a small fixed neighbourhood. No paper or project generalises it to arbitrary graphs. No checked statement on Hashlife's speed on Wireworld circuits.
- **A simulator that tracks XOR-linear (GF(2) affine) relations between state bits to replay datapaths.** The nearest is a DATE 2026 verification paper on BDD sizes for affine spaces. Nothing on simulation.
- **Emulators that detect counter loops and replace them by a closed form.** Only program-verification tools (LoAT) and one emulator story about the opposite problem.
- **Busy Beaver accelerated simulation (macro machines, proof systems).** Only secondary sources; no description of the mechanism found.
- **The game's own engine and community engines.** The store page only says "high-performance tick-based simulation" and "millions of updates per second". No technical description. Logic-simulation games (Digital Logic Sim, Turing Complete, Logic World) gave forum posts only. Redpiler (Minecraft redstone) was the one engine with documented passes (C4 uses it).
- **perfect6502 / visual6502 speed techniques.** Switch-level simulators; the one paper on speeding them up (Wojtowicz 2015, Java, 3 to 4 times) could not be fetched (certificate error); its techniques are unknown to me.
- **Dedup (ASPLOS 2024) and ARMOR (arXiv 2608.08462).** Both reduce *code footprint* for large designs whose simulator code does not fit the caches. Here the code is small and the data is in the first-level cache; nothing transfers.
- **GPU and FPGA simulators (GATSPI, Manticore, Parendi).** All need large designs or high activity; GATSPI says its largest gains are at long periods of high activity. Nothing at 2,000 elements and 0.2% activity.
- **Bitmap dirty sets, calendar queues, timing wheels for the main queue.** With unit delay and about 7 woken gates per tick, the two-list scheme v2 uses (a list plus a "queued" flag, effectively a Briggs-Torczon sparse set) is already the minimum. A bitmap of 2,000 bits costs a 33-word scan per tick. Timing wheels only matter with multi-tick delays (C4).
- **SIMD gather/scatter for count updates.** The machine is arm64 (Apple M5); NEON has no gather or scatter, and 6 to 8 flips per tick with fan-out about 3 is too few lanes anyway.
- **Prefetching or learned prediction of the next table entry.** Lookups are about 2% of the time and the hot data is in the first-level cache.
- **Run-ahead in emulators (RetroArch).** It runs frames ahead and rolls back to hide input latency; it costs throughput (one report: 1,500 to 440 frames per second). Not a speed technique.
- **Model-checking reductions (partial-order, symmetry).** They prune nondeterministic interleavings; a deterministic simulation has none.
- **Adapton and self-adjusting computation.** The sources stress that change propagation helps only when the computation is stable under input changes, and that for many changes it is no faster than recomputing; the per-node dependency graph is the attempt-12 bookkeeping in a general form. I did not find a figure for per-node overhead.
- **Verilator's V3Table, V3Life, V3Gate.** No pass-level documentation found beyond one-line descriptions (statement tables, redundant assignment removal, buffer/inverter removal). All assume zero-delay cycle semantics; buffer removal changes timing under unit delay.
- **GSIM's techniques other than its activity flags.** Node merging, bit-level splitting and reset hoisting are for word-level RTL in a cycle simulator; the gate-level analogues are done (dead logic, single-writer merging) or closed (partitions).

Not managed in this pass: the full texts of Lewis 1991, Maurer 1992 (parallel technique), Lee and Maurer 1996, Weise 1991, the Dedup paper, CXXRTL's and Icarus vvp's internals, and the ESSENT DAC 2020 paper's mux-shadow passage. Icarus vvp and CXXRTL were not searched in depth.

---

## Sources

| Source | What it is | How much I read |
|---|---|---|
| https://arxiv.org/html/2508.02236v1 | GSIM, RTL simulator with supernodes and activity bits (2025) | The technique and results sections, through a summary of the full HTML |
| https://johnhw.github.io/hashlife/index.md.html | Annotated Hashlife implementation | The whole page, through a summary |
| https://en.wikipedia.org/wiki/Hashlife | Hashlife overview | Search snippet only |
| https://cecs.uci.edu/~papers/compendium94-03/papers/1994/iccad94/pdffiles/04b_3.pdf | Maurer, The Inversion Algorithm for Digital Simulation, ICCAD 1994 | Read most of the paper (text extracted locally) |
| https://baylor-ir.tdl.org/handle/2104/5449 | Maurer, Two new techniques for unit-delay compiled simulation (PC-set, parallel technique) | Abstract via search snippets; page returned 403 |
| https://www.bohrium.com/paper-details/two-new-techniques-for-unit-delay-compiled-simulation/812462248751529986-2522 | Summary of the same | Search snippet only |
| https://research.cs.wisc.edu/multifacet/papers/pldi01_facile.pdf | Facile, PLDI 2001 | Read the action cache, miss recovery, binding-time and results passages (text extracted locally) |
| https://www.cs.cmu.edu/~bryant/pubdir/ismvl94.pdf | Bryant, ternary modelling and symbolic trajectories, ISMVL 1994 | Read the introduction and model section (text extracted locally) |
| https://github.com/MCHPR/MCHPRS/blob/master/docs/Redpiler.md | Redpiler, the Minecraft redstone compiler's design doc | The whole page, through a summary |
| https://www.microsoft.com/en-us/research/?p=151132 | Weise, Seligman, Au: compiled simulations by partial evaluation (1991) | Abstract only |
| https://scilit.com/publications/957669e4b61ed173acfb3309f0ee7e81 | Lewis, Hierarchical compiled event-driven logic simulation (ICCAD 1989; TCAD 1991) | Abstracts via search snippets |
| https://www.cs.mcgill.ca/~carl/cidvs.pdf | A paper citing compiled event-driven simulation | Search snippet only |
| https://cdnc.itec.kit.edu/downloads/06_Fault_Simulation.pdf | KIT lecture on fault simulation (concurrent fault simulation) | Search snippets only |
| https://apps.dtic.mil/sti/pdfs/ADA128019.pdf | 1980s DTIC report on fault simulation | Search snippet only |
| https://arxiv.org/abs/2011.13127 | Xu and Kjolstad, Copy-and-patch compilation, OOPSLA 2021 | Abstract and search snippets |
| https://docserv.uni-duesseldorf.de/servlets/DerivateServlet/Derivate-32715 and https://unpaywall.org/10.1145%2F1565824.1565827 | Bolz et al., Tracing the meta-level, ICOOOLPS 2009 | Abstract only |
| https://sources.debian.org/src/qemu/1%3A10.0.3%2Bds-3/docs/devel/tcg.rst | QEMU TCG developer docs, translation-block chaining | Search summary of the chaining section |
| https://arxiv.org/pdf/1104.3119 | Laarman, van de Pol, Weber, Parallel recursive state compression for free (tree compression) | Abstract and search snippets |
| https://web.mit.edu/spin_v6.4.7/Doc/Pan.html | SPIN's pan options (COLLAPSE, hash-compact) | Search snippet only |
| https://www.cs.utexas.edu/~hunt/fmcad/FMCAD11/papers/47.pdf | FMCAD 2011, ternary simulation in reachability | Search snippet only |
| https://core.ac.uk/download/87651474.pdf | Trace-level reuse (González, Tubella, Molina) | Search snippet only |
| https://ftp.cs.wisc.edu/sohi/papers/1997/isca.inst-reuse.pdf | Sodani and Sohi, Dynamic instruction reuse, ISCA 1997 | Search snippets only |
| https://www.informs-sim.org/wsc17papers/includes/files/077.pdf | Winter Simulation Conference 2017, time-parallel simulation and state matching | Search snippets only |
| https://www.intel.cn/content/dam/develop/external/us/en/documents/cgo07-256665.pdf | Wallace and Hazelwood, SuperPin, CGO 2007 | Search snippets only |
| https://www.comp.nus.edu.sg/~wongwf/papers/CGO08.pdf | PiPA, pipelined profiling, CGO 2008 | Search snippet only |
| https://escholarship.org/content/qt0t72f27j/qt0t72f27j.pdf | Wang, Nijssen, Beamer, Don't Repeat Yourself (Dedup), ASPLOS 2024 | Search snippets only |
| https://danglingpointers.substack.com/p/dont-repeat-yourself-coarse-grained | Blog summary of Dedup | Search snippet only |
| https://arxiv.org/pdf/2608.08462 | ARMOR, node compression for RTL simulation | Search snippet only |
| https://woset-workshop.github.io/PDFs/2021/a23-slides.pdf | ESSENT slides (mux shadows, branch hints) | Search snippet only |
| https://escholarship.org/uc/item/1np6w6rf | WordRev, HOST 2013 | Abstract only |
| https://arxiv.org/pdf/2303.02762 | Word-level models from LUT netlists | Search snippet only |
| https://agra.informatik.uni-bremen.de/doc/konf/DATE2026_CD.pdf | DATE 2026 proceedings, affine-space BDD verification | Search snippet only |
| https://ieeexplore.ieee.org/document/480147 | McGeer et al., decision diagram evaluation, ICCAD 1995 | Abstract via search snippets |
| https://arxiv.org/pdf/2001.01516 | A calculus for modular loop acceleration | Search snippet only |
| https://arxiv.org/pdf/2202.04546 | LoAT system description | Search snippet only |
| https://arxiv.org/pdf/2105.06712 and https://www.csd.cmu.edu/sites/default/files/phd-thesis/CMU-CS-05-129.pdf | Self-adjusting computation (Acar and later work) | Search snippets only |
| https://www.cs.umd.edu/projects/PL/adapton/ | Adapton artifact page | Search snippet only |
| https://en.wikipedia.org/wiki/Zobrist_hashing | Zobrist hashing | Search snippet only |
| https://blog.acolyer.org/2015/11/23/hashed-and-hierarchical-timing-wheels/ | Summary of Varghese and Lauck, timing wheels | Search snippet only |
| https://arxiv.org/html/2403.04714v1 | Parendi, thousand-way parallel RTL simulation | Search snippet only |
| https://arxiv.org/pdf/2301.09413 | Manticore | Search snippet only |
| https://arxiv.org/pdf/2203.06117 | GATSPI, GPU gate-level simulation | Search summary of results and limits |
| https://docs.libretro.com/guides/runahead/ | RetroArch run-ahead | Search summary |
| https://store.steampowered.com/app/1885690/Virtual_Circuit_Board/ | The game's store page | Search snippet only |
| https://ejournals.eu/en/journal_article_files/full_text/018eced7-3582-71a2-837e-05fc7240f470/download | Wojtowicz, transistor-level 6502 speedups (2015) | Not read: certificate error; abstract via snippets |
| https://rust-digger.code-maven.com/src/perfect6502-sys-0.2.2/perfect6502/README.md | perfect6502 README | Search snippet only |
