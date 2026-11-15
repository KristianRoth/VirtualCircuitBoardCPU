# VCB engine v1 (reference engine)

## Problem & goal
Three documents describe the engine:
- `vcb-engine.md` (**the framework spec**): goals, CLI, stop rules, outputs and the test ladder.
- `vcb-game-facts.md` (**the facts file**): what is known about the game, and every open question about it.
- This file: the design of **v1**, the single-threaded, readable reference engine, and the workflow for proving it against the game.

v1 is done when:
1. every ink is **verified** or **modelled** (see the gate below),
2. ladder rungs A–D pass, and
3. the user's CPU runs test programs and stops on a Breakpoint at the same tick as the game, with the same memory.

Speed is not a goal.

### Terms
- **Rungs:** A = one ink group per board, B = small combinations, C = VMem boards, D = the CPU. Rung E (AoC) is outside v1.
- **Entity:** a group of pixels simulated as one thing. There are four kinds: trace, component, bus, VMem pin.
- **Persistent region:** the VMem address range From..To in `vmem_settings`. The game copies it into the project when the simulation ends, so a save made afterwards holds what the circuit wrote.
- **Question N:** open question N in the facts file. All open questions about the game live there and nowhere else.
- **Model:** a rule taken from the manual or assumed, not yet observed in the game.

### Overrides of the framework spec
Where the two disagree, this list wins for v1:
- One Cargo package (library + binary), not a workspace.
- Claude generates the test boards; the user helps where needed. The framework says the user builds them.
- One board per ink group, not one ink per board.
- `--timer-ticks` replaces `--timer-interval`. The file's `timer_interval` (milliseconds) is read and not used.
- External VMem (`.vcbmem`) is supported.
- VMem fixtures use an "after" save, not VMem words typed into the TOML.
- The openVCB patch and benchmark script come after v1.
- Address watch, `--quiescent`, the TOML config file, Ctrl-C handling and progress output are milestone M7, after the CPU rung.
- `state()` covers traces, components and pins, in canonical entity order.
- Rung C drops "a counter that stores into VMem" (rung D covers it) and adds locking, content precedence and narrow content width.

## Users & roles
- **The user** opens generated boards in the game (Update 1.0.1), fills in checklists, makes "after" saves, and edits the CPU so test programs run without mouse clicks.
- **The engine author (Claude)** generates boards, writes the engine, and turns observations into fixtures.

## Scope
### In scope
- One Cargo package in `engine/`: library (`src/lib.rs`) and a thin binary `vcb` (`src/main.rs`). Rust stable, pinned in `engine/rust-toolchain.toml`.
- `.vcb` reader **and writer** (the writer is for generated boards and tests).
- Text-art boards and `vcb gen`.
- Netlist builder for every ink in the facts file's colour table.
- Dirty-list simulation core and VMem.
- Stops and outputs needed by the ladder: `--ticks`, Breakpoint, `--no-breakpoint`, `--dump-vmem`, `--watch`, `--seed`, `--clock-interval`, `--stats` (ticks, time, TPS), `--force`.
- Debug aids: `--probe` and `--render`.
- External VMem (`.vcbmem`) reading.
- The verified-ink gate.
- Ladder rungs A–D. Every ink goes through rung A before the VMem and CPU rungs, including the inks with logic that the CPU doesn't use (mesh, timer, random, wireless).
- M7: the remaining framework features (see Overrides).

### Out of scope
- Everything the framework spec lists as out of scope or Later (assembler, v2, VInput, VDisplay, viewer, `.tres`).
- Rung E (AoC).
- The openVCB patch and benchmark script.
- Decoration layers, `led_palette` colours, blueprint strings.

## Design
### Modules
| Module | Job |
|---|---|
| `ink` | `Ink` enum, the colour→ink table, the gate table |
| `format` | blob encode/decode, `.vcb` JSON read/write, `.vcbmem` read, all reader errors |
| `board` | `Board`: width, height, one `Ink` per pixel, settings, VMem words |
| `textboard` | parse a text-art board into a `Board` |
| `netlist` | `Board` → `Components`: entities, connections, pixel→entity map |
| `sim` | `trait Simulation`, the v1 engine, stop conditions |
| `vmem` | pin latches, read/write/lock timing |
| `render` | PNG snapshot |
| `main` | CLI only: argument parsing, output |

`Ink` keeps the colour: `Trace(0–15)`, `Bus(0–5)`, `Wireless(0–3)`. The facts file's colour table is canonical; a test checks the `ink` table against `saves/all-inks.vcb`.

### Netlist
Every rule below is a **model** until a fixture observes it. Pixels connect only to their four direct neighbours.

**The key of a pixel** is what "same ink" means in rules 4–7: its exact ink, including the colour for a trace pixel. Read and Write are their own keys. Bus pixels of any colour share one key. This is the model for question 12.

Step 1, pins. When VMem is enabled, each pixel at a pin position from `vmem_settings` becomes a **pin** entity of its own, starting off. A pin never merges with a neighbouring latch or pin. A pin size other than 1×1, a pin outside the board, or two pins on one pixel is an error naming the pin.

Step 2, adjacency.
1. **Traces:** touching pixels of any trace colour, Read or Write form one trace entity.
2. **Components:** touching pixels of the same component ink form one component. Different inks never merge. All pixels of one Wireless ink on the board are one component.
3. **Buses:** touching bus pixels of any colour form one bus entity.

Step 3, joins that depend only on pixels (any order):

4. **Cross:** the pixel left of a cross pixel is joined with the pixel right of it, and the one above with the one below, when both are the same kind (trace with trace of any colour, bus with bus, component with the same component ink). Two touching cross pixels are an error naming the pixel, because the game calls that invalid.
5. **Tunnel:** for a tunnel pixel P and a direction d, let N be the pixel on the side of P opposite to d. If N is a trace, component or bus pixel, scan from the pixel after P in direction d. At the first tunnel pixel Q whose neighbour in direction d has the same key as N, join N with that neighbour and stop. If the board edge is reached, nothing is joined.
6. **Mesh:** take every trace, component and bus pixel that touches a mesh pixel anywhere on the board. Pixels with the same key are joined.

Step 4, bus (after step 3, because steps 4–6 can merge buses):

7. **Bus:** for each bus entity, take every trace and component pixel that touches it. Pixels with the same key are joined.

Step 5, connections:

8. A component or pin adjacent to a Write pixel writes that pixel's trace. A component or pin adjacent to a Read pixel reads that pixel's trace. A component and a trace are connected at most once in each direction, however many pixels join them. A Read or Write pixel touching two components connects both (question 20).
9. Annotation, Filler and empty pixels are ignored.

Joining uses a union-find. Any pattern these rules don't define is an error naming the pixel; the builder never guesses. `Components` numbers entities in order of their top-left-most pixel (row first, then column); that is the canonical order for `state()`.

### Simulation core: dirty list + recompute
**State**
- Per trace: `on` (any writer is on) and `rose` (it went from off to on in the most recent Apply step).
- Per component and pin: its output bit.
- Global: the tick number, the random generator, VMem and its `locked` flag.

**Tick 0** is the reset state, matching the game's counter (observed on the `gates` board). Every component has the output it gets with all inputs off: Latch On and the inverting gates (NOT, NOR, XNOR, and NAND when it has inputs) start on, everything else off. Traces are then computed from their writers. A trace that is on at tick 0 has `rose` set and its readers are on the dirty list, so a latch reading it toggles at tick 1.

**One tick**
0. **Schedule.** If the tick number is a multiple of `clock_interval`, every Clock is marked to flip. The same for Timer with `--timer-ticks`. Clock and Timer ignore their inputs (model for question 13: the first flip is at tick `clock_interval`).
1. **Evaluate.** For every component and pin on the dirty list, compute its new output from the trace states left by the previous tick. Nothing changes yet. While VMem is locked, pins are not evaluated.
2. **Apply.** Clear every `rose` flag. Flip the components and pins whose output changed, and the scheduled clocks and timers. Recompute each trace they write and set `rose` on traces that turned on. Put the readers of every changed trace on the dirty list for the next tick. Remember which pins flipped.
3. **VMem.** See below.
4. **Stops.** Check the stop conditions as in the framework spec.

A component is never on the dirty list twice. This gives the observed one-tick gate delay.

**Component rules** (A = active inputs, T = total inputs)

| Ink | Output |
|---|---|
| Buffer, OR, LED, Breakpoint, Wireless | A > 0 |
| AND | A > 0 and A == T |
| XOR | A odd |
| NOT, NOR, XNOR | the inverse of Buffer, OR, XOR |
| NAND | A < T. With no inputs it is off, so it is not the exact inverse of AND (observed) |
| Latch On / Off, pin | toggles once per input trace that has `rose` set, so two inputs rising together cancel out (observed) |
| Clock | flips on ticks that are multiples of `clock_interval` |
| Timer | flips on ticks that are multiples of `--timer-ticks`; never game-exact |
| Random | if at least one input trace has `rose` set, draws once and toggles with probability ½ (model for question 14) |

- **Breakpoint** reports a stop on the tick its own output goes from off to on. The engine's stop tick must equal the counter the game shows at the pause (question 17).
- **Random generator:** the user records about 64 ticks of a Random pixel for seeds 1 and 2. In one session Claude tries Godot's `RandomNumberGenerator` seeded with `random_seed`, taking the low bit of `randi()`, the high bit of `randi()`, and `randf() < 0.5`. If one reproduces both sequences, Random is exact. If none does, v1 uses its own seeded generator and Random is "modelled". The result of each candidate is recorded in the facts file.

### VMem
- 2^20 big-endian 32-bit words, loaded from the inline blob, or from `<project>.vcbmem` when `vmem_is_external` is true. The `.vcbmem` must be exactly 4,194,304 bytes; the inline blob is then ignored.
- Address bit k and content bit k are pins at `(x − offsetX·k, y + offsetY·k)`.
- Step 3 of the tick, in order (model, from the manual and the facts file's observations; questions 15, 16 and 21):
  1. `prev_address` is the address pins' value at the end of the previous tick.
  2. If any content pin was flipped by the circuit in this tick's Apply, write the content pins' value to the word at `prev_address`. Bits above the content width are written as 0.
  3. If any address pin flipped in this tick's Apply, load the word at the new address into the content pins. Pins whose value changes go through the same path as Apply: their traces are recomputed, `rose` is set, and readers are put on the dirty list. Readers therefore see the loaded word on the next tick.
  4. If an address pin flipped, VMem is locked for the next tick: pins are not evaluated, so input to them is lost. The lock clears after that tick.
- Word 0: zeroed on load with a warning if the file had a non-zero value, until question 8 is answered.

### The gate: verified, modelled, unverified
The gate works on **units**: each ink kind with behaviour (Trace, Read, Write, Cross, Tunnel, Mesh, Bus, each of the 8 gates, Latch On, Latch Off, Clock, Timer, Random, LED, Breakpoint, Wireless) plus the feature `vmem`. Annotation and Filler are exempt.

| Status | Meaning | `vcb run` |
|---|---|---|
| unverified | no observed fixture covers all of the unit's rules | fails: `<unit> at x,y is not verified yet` |
| verified | observed fixtures cover every rule and question listed for the unit | runs |
| modelled | cannot match the game tick for tick; a fixture checks only what can be observed | runs and prints `<unit> is modelled, not game-exact` |

- Timer is always modelled. Its fixture checks that a Timer drives a trace like any component (observed) and that it flips every `--timer-ticks` ticks (engine only).
- Random is verified if a generator candidate matches. Otherwise it is modelled, and its fixture checks that the output changes only on the tick after an input rose, and that a fixed seed repeats.
- The `ink` module holds one table: unit → status → the rule and question ids it needs.
- Every `expected.toml` has `covers = [...]` naming netlist rules (`rule1`…`rule9`), component rows and question numbers. A test fails if a verified or modelled unit has a needed id that no fixture covers.
- `--allow-unverified` runs anyway. It prints one warning per unverified unit and adds `(unverified: …)` to the `stopped at tick` line.

## Interface
Additions to the framework CLI:
```
vcb info <board.vcb>              # size, settings, pixel count per ink, entity counts, gate status per unit
vcb info --adjacency <board.vcb>  # count of each pair of touching inks that involves Read, Write, Cross, Tunnel, Mesh or Bus
vcb gen <board.txt> -o <out.vcb>  # also writes <out>.checklist.md and <out>.png
vcb run <board.vcb> ...
  --probe <x,y>[=<label>]         # repeatable; prints the on/off state of the entity at that pixel
  --render <tick>:<file.png>      # repeatable; snapshot after that tick
  --timer-ticks <n>               # required when the board has Timer ink
  --allow-unverified
```
- **Probe output:** one line per tick on stdout: `tick 7: a=1 b=0 led=1`. A probe on a pixel with no entity, or a label used twice, is an error.
- **Render:** PNG cropped to the bounding box of the drawn pixels. An ink keeps its file colour when on and has each channel halved when off; inks with no state keep their colour. The scale is 8 pixels per board pixel when the box is at most 256 pixels on its longer side, otherwise 1. A render tick after the run's last tick is an error naming the tick. Uses the `png` crate.

### Text-art boards
- A text file: header lines `key = value`, a blank line, then the art with one character per pixel. The art is placed at board position 16,16 on a 2048×2048 board.
- Header keys: `labels = on` (draws each probe's number below its LED block in Annotation ink, so it can be read in the game), `clock_interval`, `random_seed`, `vmem` (address and content pin settings), `persistent` (From, To), `word.<address>` (VMem preload), `probe.<label> = col,row` (relative to the art), `ticks = …` (the ticks the checklist asks for), `covers = …`. The writer always sets `random_is_time_seed` to false and leaves `assembly` empty.
- The character legend is one table in `textboard`, with a character for every ink in the colour table (test). An unknown character is an error naming the line and column.
- Sources live in `engine/boards/` and are committed when written.

## Fixtures and the observation workflow
1. Claude writes `engine/boards/<name>.txt` and runs `vcb gen` into `saves/pending/` (git-ignored). This produces `<name>.vcb`, `<name>.checklist.md` and `<name>.png`.
2. **The checklist** has a header with the board file's SHA-256, then one row per probe and listed tick with an empty on/off box, a field for the tick counter at any Breakpoint pause, and a field for the counter when the user stopped. It carries no engine prediction (a test checks this).
3. **Finding and reading probes:** every probed signal drives its own LED block of at least 3×3 pixels, and the LED blocks sit in one row in checklist order. The PNG shows the board with each LED block numbered, and the checklist gives each one's board `x,y`. A lit LED is unambiguous.
4. **Budget:** at most 24 probes and 16 listed ticks per board. Questions of the form "the first tick at which X turns on" are preferred to full tables.
5. The user opens the board, steps (the game has a step mode and a tick counter that starts at 0), and fills in the checklist. For VMem boards the persistent region is already set by `vcb gen`; the user runs until the board's own Breakpoint pauses it or its LEDs stop changing, returns to edit mode, and saves `<name>.after.vcb` under a new filename.
6. Claude imports the result into `engine/tests/boards/`: the `.vcb`, the filled checklist as `<name>.observed.md` (with the observer, date and platform added), and for VMem boards `<name>.after.vcb`. The fixture test reads the observed checklist directly, so there is no separate `expected.toml` and no hand transcription. A line `- ignore LEDs: n` in the checklist excludes a probe, and must be followed by a line saying why. The import refuses an "after" save whose logic layer or `vmem_settings` differ from the generated board. The gate table is updated in the same commit.
7. **Disagreement:** if the engine disagrees with an observation, the user first re-observes only the disputed probes and ticks from a fresh blank checklist. Two matching observations stand, and then the engine is the one that is wrong. A model change must keep every earlier fixture passing, and the changed rule is edited in this spec and the facts file in the same commit.
8. Only observed fixtures are in `engine/tests/boards/`.

Sources of truth: the `.txt` for layout and probe positions; the observed checklist for probe values; the "after" save for VMem and the persistent range.

Fixture checks (tests):
- **Probe fixtures:** the engine's probe values equal the observed values at every listed tick.
- **VMem fixtures:** after the stop rule, the engine's VMem equals the "after" save's VMem over the persistent region, and at least one word in that region differs between the board and its "after" save. Until the To question (10) is answered, test data stays off the To address.
- **Reproducible boards:** each committed `.txt` parses to the same pixels and settings as the committed `.vcb`, and that file's SHA-256 equals the one in `<name>.observed.md`.
- **Game version:** every fixture's `game_version` equals the `GAME_VERSION` constant.
- **CPU coverage:** every pair of touching inks reported by `vcb info --adjacency` for the CPU board also occurs in an observed fixture.

### CPU fixtures (rung D)
- The CPU board changes over time. Each fixture commits the copy of the CPU it was observed on.
- The user edits the CPU so that it starts and runs with no mouse clicks: debug switches are fixed in ink (Latch On or Latch Off), and the program ends by tripping a Breakpoint pixel.
- A script in `engine/tools/` writes each test program into `vmem_data`, clears the `assembly` field (so the game doesn't assemble over it, question 19) and sets the persistent region to cover every address the program writes. The instruction encoding comes from the repo's `README.md` and the `symbol`/`macro` lines of the `.vcbasm` files.
- The first CPU fixture is a probe checklist only: the clock, the VMem address pins and a few program-counter bits for the first 16 ticks after reset.
- Program fixtures record the counter at the Breakpoint pause. The engine must stop on `breakpoint` at that tick with equal memory over the persistent region.
- **When a CPU fixture fails:** the engine prints the probe values; the user steps the game to half the failing tick and compares the same probes; repeat on the half that differs until the first differing tick is found.

## Edge cases
- A component with no inputs: AND is off, NAND is off, NOT is on (observed).
- Read or Write touching a tunnel, mesh or bus: matched by its own key (model, question 12).
- `clock_interval` or `--timer-ticks` of 0 is an error.
- `vcb gen` checks all three outputs before writing any. It never overwrites an existing checklist, even with `--force`, unless the new one is identical.
- Every output path of a run must differ from every file the run reads (the `.vcb`, its `.vcbmem`, the gen source) and from every other output, even with `--force`.
- A render larger than 64 MiB of pixels is an error.

## Errors
As in the framework spec: one stderr line, non-zero exit. New messages name their item: unverified unit (`x,y`), touching cross pixels (`x,y`), undefined netlist pattern (`x,y`), pin problems (pin name and `x,y`), missing or wrong-sized `.vcbmem` (path), missing `--timer-ticks`, zero interval, unknown text-art character (line:column), probe on an empty pixel (`x,y`), duplicate probe label, render tick after the run, output path collision (path).

## Security & privacy
- Inputs are trusted local files, decoded with bounds as in the framework spec.
- The repo is public. `manual/` (the game's copyrighted text) is git-ignored, and commits stage explicit paths.
- Fixtures record the observer as a role (`user`), not a name.

## Performance
No target. `--stats` reports TPS so the CPU's v1 speed is known when v2 starts.

## Acceptance criteria
Reader, writer, tools
- [ ] `vcb info` on `32-bit-alu-try.vcb` and on `saves/all-inks.vcb` equals a committed expected output (2048×2048; 40 and 48 colours).
- [ ] Writing a `Board` and reading it back gives an equal `Board`, including trace and bus colours. `saves/all-inks.vcb` read and re-written has an identical logic layer.
- [ ] The framework's "Reader and errors" criteria pass. Each new error message above has a test for a non-zero exit, exactly one stderr line, and the named item.
- [ ] A board with `vmem_is_external: true` and a `.vcbmem` next to it loads that memory: `--dump-vmem` at `--ticks 0` equals the file's words, except word 0.
- [ ] `vcb gen` on a sample board gives each legend character's ink at `16+col, 16+row`, the header's settings, and each probe at the expected board pixel.
- [ ] The generated checklist contains no on/off values.
- [ ] `--render` on a two-buffer board: the on buffer has its file colour, the off buffer has each channel halved, and the PNG size is bounding box × scale.
- [ ] `--probe` prints one line per tick in the documented format.

Gate
- [ ] A board with an unverified unit fails with `<unit> at x,y is not verified yet`. With `--allow-unverified` it runs, warns once per unit, and marks the stop line.
- [ ] A board with VMem enabled fails the same way until `vmem` is verified.
- [ ] A board with Timer ink fails without `--timer-ticks`, and with it prints `Timer is modelled, not game-exact`.
- [ ] The coverage test passes: every verified or modelled unit has all its needed ids covered by fixtures.

Engine
- [ ] Every fixture check listed under "Fixture checks" passes.
- [ ] Observed fixtures exist for: a gate joined to one trace by two Read pixels counting as one input; a trace crossing a component at a Cross; a bus through a Cross and through a Tunnel; a two-colour trace on one bus; three tunnel pixels in a row; a no-input AND and NAND; a Write pixel touching two components; Latch On driving a Buffer and a Latch from tick 0; a Breakpoint behind a buffer chain of known length with the counter at the pause recorded.
- [ ] With a fixed seed, two runs of every fixture and of the CPU board for 10,000 ticks give identical `state()` after every tick and identical VMem.
- [ ] `saves/vmem-preserve-before.vcb` and `saves/vmem-preserve-after.vcb` are the first VMem pair fixture (word 1 = 3).
- [ ] VMem timing fixtures also have probe checklists on the address pins, the content pins and a Buffer behind a content pin, around an address change, a write while locked, and a simultaneous address and content toggle.
- [ ] Rung D: the first-fetch probe fixture passes, and at least two test programs (store a constant; a loop of 10) stop on `breakpoint` at the tick recorded from the game with equal memory over the persistent region.
- [ ] M7: the framework's "Stops and outputs" criteria pass.

## Rollout
Each milestone ends with a local commit once its tests pass. Commits stage explicit paths.

| # | Milestone | Needs from the user | Status |
|---|---|---|---|
| M0 | `.gitignore` (`manual/`, `.DS_Store`, `saves/pending/`, `engine/target/`, `*.vcb.tmp`); first commit of `specs/`, the `.vcb` saves and the engine skeleton; `format` reader/writer, `ink`, `vcb info`, reader error tests | open one written board and one re-written copy of the CPU in the game and confirm they load | done 2026-11-15 |
| M1 | `textboard`, `vcb gen`, netlist steps 2 and 5 (rules 1, 2, 8, 9, without wireless), the core, `--probe`, `--render`, the gate; board **gates**: Read/Write, connected-once rule, 8 gates, Latch On/Off, LED, first tick, tick 0 | observe 1 board | done 2026-11-15 (fixtures `gates`, `gates-recheck`) |
| M2 | Netlist rules 3–7 and wireless; `vcb info --adjacency`; boards **routing** (cross, tunnel, bus, mesh, wireless, colour cases, and every ink pair the CPU census shows) and **general** (clock intervals 1–3, clock with an input, breakpoint counter, latch edge cases) | observe 2–3 boards | next |
| M3 | Board **random-timer**. Exit: Timer is modelled; Random is verified or modelled after the one-session generator search. This milestone cannot block M4–M6 | observe 1 board |  |
| M4 | Rung B on one board, within the checklist budget: ring oscillator, SR latch, D flip-flop, 4-bit counter, 4-bit adder with 4 input pairs | observe 1 board |  |
| M5 | Rung C: pins, VMem write, read back, address 0, locking, content precedence, content width under 32 bits, the To-inclusive question first; `.vcbmem` reading tested without the game | run, checklist and save per board |  |
| M6 | Rung D as described under "CPU fixtures". First: question 19 (five minutes in the game) | edit the CPU to run without clicks; run and save |  |
| M7 | Address watch, `--quiescent`, TOML config, Ctrl-C, progress output | nothing |  |

## Open questions
All game questions are in the facts file. This table says which model v1 uses meanwhile and when the answer is needed.

| Question | Interim model in v1 | Needed before |
|---|---|---|
| 12 colour at cross, tunnel, bus, mesh | the key of the touching pixel; cross joins traces of any colour | M2 done |
| 13 clock phase, clock with an input | first flip at tick `clock_interval`; inputs ignored | M2 done |
| 17 counter at a Breakpoint pause | equals the tick the Breakpoint turned on | M2 done |
| 14 Random probability and draws | ½, one draw per component per tick | M3 done |
| 10 persistent To inclusive | compared range avoids the To address | first M5 board |
| 15 VMem lock ticks | the one tick after an address change; input is lost | M5 done |
| 16 tick the loaded word is visible | the tick after the address pins flip | M5 done |
| 21 content width under 32 bits | upper bits written as 0 | M5 done |
| 8 VMem address 0 | zeroed on load, warn | M5 done |
| 4 pin size other than 1×1 | error | M5 done |
| 19 assembly and Persistent Memory | the script clears `assembly` | M6 start |

Decisions still owed by the user:
- **ALU on its own** (framework rung D): add an ALU-only board before the CPU, or rely on the first-fetch fixture and the bisect procedure? Decide at M6 start.
- **Licence** for the engine code in the public repo. Decide before the first commit of `engine/`.

## Decision log
- Split the spec → `vcb-game-facts.md` (game), `vcb-engine.md` (framework), this file (v1 design).
- Unverified inks → refused until observed fixtures exist; `--allow-unverified` for pending boards.
- Who makes test boards → Claude generates as much as possible; the user helps where needed.
- Debug aids → both per-tick pixel probes and PNG snapshots.
- External `.vcbmem` → supported in v1.
- Core model → dirty list + recompute. Counters are left for v2, so the differential test compares two independent mechanisms.
- Rust → `brew install rustup`, stable 1.100.0.
- Pending fixtures → not in the repo until observed.
- Commits → locally per milestone when tests pass; never pushed by Claude.
- Ink order → all inks go through rung A before the VMem and CPU rungs.
- Observations → checklists the user fills in.
- Board packing → one board per ink group.
- Connection rules the user is sure of → no diagonals; gates need Read/Write; cross passes straight through; trace colours are visual only within a trace.
- Bus rule → the user is sure the same ink continues across a bus, so colour matters there; tested anyway (question 12).
- Stepping → the game has a step mode and a tick counter that starts at 0.
- Board generator → Rust, text-art boards.
- Timer → modelled in ticks with a required `--timer-ticks`.
- Random → try to match the game's generator in one session, then fall back to an own seeded generator.
- Manual → transcribed to `manual/*.md` from the user's screenshots; kept out of git.
- Persistent Memory → tested 2026-11-15, works; VMem fixtures are a pair of saves.
- Review (2026-11-15) → all clear fixes applied.
- Review: Timer and Random → a "modelled, not game-exact" status that passes the gate with a warning.
- Review: CPU start → the debug options are latches the user clicks today; the CPU will be edited so test programs run continuously without clicks.
- Review: framework features without a milestone → M7, after the CPU rung.
- `gates` board observed 2026-11-15 → questions 6, 11, 18 and 20 answered; tick 0, latch and NAND rules changed to match. LED 7 (AND with one input on, one off) was reported lit, which contradicts every other observation; it was re-observed on `gates-recheck` and is off, as the model says; the no-input AND and NAND are both off. The `gates` fixture skips LED 7.
- Fixture record → the filled checklist itself (`<name>.observed.md`), read by the test; no `expected.toml`.
- Verified after M1 → Trace, Read, Write, Buffer, AND, OR, XOR, NOT, NAND, NOR, XNOR, LED. Latch On/Off wait for the remaining latch cases in M2.
- Coverage test → for now it checks that every verified ink is on an observed board; the per-rule `covers` check comes with M2's routing boards.
- Boards for the user → generated into `saves/pending/`, never under `engine/`.
