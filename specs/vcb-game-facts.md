# VCB game facts

What is known about Virtual Circuit Board (Steam app 1885690) and its `.vcb` save format. This file holds facts only; what the engine does with them is in `vcb-engine.md`.

**Game version:** everything here is from VCB **Update 1.0.1** (confirmed in the game on 2026-11-13). A game update means re-checking these facts and every fixture.

**Ground truth comes from the game in two ways.** For VMem: set a Persistent Memory region, run, return to edit mode and save; the save then holds the words the circuit wrote (see Saves). For everything else (component on/off states, tick timing): the user steps the simulation and records what they see. Save files hold no other simulation state. openVCB (cloned in `openVCB/`, pinned at commit `4903fa8`) reads only the 2022 Godot `.tres` format and is an untrusted second opinion.

## File container
- A `.vcb` is JSON with `file_type: "RF_VCB_PROJECT"` and `file_version: 2`.
- The old format is a Godot resource starting with `[gd_resource`.
- `layers` holds 3 base64 blobs. `layers[0]` is the logic layer. `layers[1]` and `layers[2]` were empty in every test and are presumably decoration (open question 7).
- Other fields seen: `vmem_data`, `vmem_settings`, `is_vmem_enabled`, `vmem_is_external`, `vinput_is_enabled`, `vdisplay_is_enabled`, `clock_interval`, `timer_interval`, `random_seed`, `random_is_time_seed`, `simulation_speed_ticks` (the game's UI speed setting), `led_palette`, `assembly`.

## Blob format
Used by the layers and by `vmem_data`: base64 text, decoding to a zstd frame followed by a trailer of (tag, value) pairs. Each pair is two little-endian i32 values, and the tag is always `2`.
- Layers have three pairs (24 bytes): `(2, height, 2, width, 2, size)`, e.g. `(2, 2048, 2, 2048, 2, 16777216)`.
- `vmem_data` has one pair (8 bytes): `(2, size)`.
- `size` equals the decompressed length. One exception seen: `32-bit-alu-try.vcb` has a VMem size of 4,194,305, one byte more than normal.
- `zstd -dc` decompresses a blob but warns about the trailing bytes.

## Logic layer
- Width×height RGBA pixels (2048×2048 in `32-bit-alu-try.vcb`). The alpha channel is always `ff`; `00000000` is empty.
- **Pixels are ink types, not state** (verified with `saves/first.vcb.tmp` against `saves/first_2.vcb`, the same sample saved stopped and running): the logic layer and `vmem_data` were byte-identical.
- **Generated files load in the game:** a `.vcb` written by script opened in the game, and the game's re-save had identical pixels.

### Colour→ink table
Complete for Update 1.0.1. Confirmed by the user against `saves/all-inks.vcb` (one pixel of each of the 48 inks), consistent with the 2-bit counter sample, and it covers all 40 colours in `32-bit-alu-try.vcb`.

| Colour(s) | Ink |
|---|---|
| `2a3541` `9fa8ae` `a1555e` `a16c56` `a18556` `a19856` `99a156` `88a156` `6ca156` `56a18d` `5693a1` `567ba1` `5662a1` `6656a1` `8756a1` `a15597` | Trace (16 user-chosen colours, all the same ink) |
| `66788e` | Cross |
| `535572` | Tunnel |
| `646a57` | Mesh |
| `7a2f24` `3e7a24` `24417a` `25627a` `7a2d66` `7a7024` | Bus (6 bus colours) |
| `4d383e` | Write |
| `2e475d` | Read |
| `92ff63` / `ffc663` / `63f2ff` / `ae74ff` | Buffer / AND / OR / XOR |
| `ff628a` / `ffa200` / `30d9ff` / `a600ff` | NOT / NAND / NOR / XNOR |
| `63ff9f` | Latch On (a latch that starts on) |
| `384d47` | Latch Off (a latch that starts off) |
| `ff0041` / `ff6700` / `e5ff00` | Clock / Timer / Random |
| `ffffff` | LED (with the default `led_palette`) |
| `e00000` | Breakpoint |
| `ff00bf` `ff00af` `ff009f` `ff008f` | Wireless 0–3 |
| `3a4551` | Annotation (no logic) |
| `8caba1` | Filler (no logic) |

Changing `led_palette` probably only affects display colours; unverified.

## Simulation
- **Gate delay is 1 tick:** in a chain of buffers, each turns on one tick after the previous one (observed while stepping `saves/vmem-test3.vcb`).
- A latch's starting value is part of its ink (Latch On or Latch Off). Latches toggle on input edges.
- The game always simulates from reset; it never loads live state.
- The game has a step mode and a tick counter that starts at 0 (reported by the user, 2026-11-15).
- The manual's rules for every ink are under "What the User Guide says". What is still unobserved is in questions 3 and 6.

### Observed on the `gates` board (2026-11-15, `engine/boards/gates.txt`)
The user stepped ticks 0–6 and listed the lit LEDs. Each LED is one tick behind the signal it shows.
- **Tick 0 already has outputs.** Nothing is lit at tick 0, but Latch On and the inverting gates are already on at tick 0 and so are their traces: an LED on a Latch On trace, and an LED on a NOT with no input, both light at tick 1. (Answers questions 6 and 18.)
- **Inverting gates start on, then react.** NOT, NOR and XNOR with one input on were lit at tick 1 only: they start on (all inputs off) and turn off one tick after seeing their input.
- **Gate delay is one tick** per component (Latch On → Buffer → LED lights at tick 2).
- **Trace colours merge**, and a latch touching a trace with no Write pixel drives nothing.
- **Gate rules match the manual** for Buffer, OR, XOR, NOT, NAND, NOR, XNOR with inputs (on, off) and for AND, XOR, XNOR with (on, on).
- **No inputs:** NOT is on; AND is off; NAND is **off**. So NAND behaves as "A < T", not as the exact inverse of AND.
- **Connected once:** an XOR joined to one trace by two Read pixels counts one input (it was on).
- **A trace that is on at tick 0 is a rising edge for a latch:** a Latch Off reading a Latch On's trace, or a NOT's trace, turned on at tick 1. (Question 18.)
- **Latch, two inputs rising in the same tick:** no change. The latch toggles once per rising input. (Question 11, first case.)
- **One Read pixel touching two LEDs** lights both. (Question 20, for Read.)
- **Re-check** (`engine/boards/gates-recheck.txt`, same day): AND with (on, off) and with (off, on) is off; AND (on, on) is on; NAND (on, off) is on; AND and NAND with no inputs are off; NOT with no inputs is on. The first report of LED 7 on the `gates` board (AND with one input on, lit) was a slip.

### Tunnel compile errors (observed 2026-11-15, first version of `routing-a`)
The game refused to simulate the board and listed these in the Event Log:
- **"Unmatched tunnel (left to right / right to left). No exit tunnel with the same ink as the entrance was found until the edge of the board."** Reported for a colour-2 trace entering a tunnel whose only exit had a colour-9 trace behind it, and for trace–tunnel–trace in a row with no second tunnel. So **"same ink" at a tunnel means the same trace colour**, and an entrance without an exit is an error, not an open end.
- **"Unexpected tunnel entrance. While looking for a tunnel exit another entrance for the SAME ink was found."** Reported when a right-to-left scan for colour 9 reached another colour-9 entrance first. Scans run along the whole row, so tunnels of unrelated circuits on one row see each other.
- A tunnel pixel with ink on one side is an entrance for the direction away from that ink. The error's coordinates are the ink pixel behind the entrance (unmatched) or the entrance that was found (unexpected).
- A colour-2 pair with a colour-9 entrance between its two tunnels gave no error.
- All tunnels in `32-bit-alu-try.vcb` satisfy these rules (the engine's netlist builds it with the same checks).

### Observed on the routing and general boards (2026-11-15)
Boards `routing-a`, `routing-b`, `general` (clock interval 3) and `general-1` (clock interval 1), sources in `engine/boards/`. Every LED matched the model except the clock's starting state.
- **Cross:** the horizontal and vertical traces stay separate. A cross joins traces of different colours. Write and Read pixels count as trace pixels at a cross. A gate split by a cross is one gate (OR, XOR, XNOR, AND).
- **Tunnel:** the colour-2 pair connected and the colour-9 pair stayed separate while interleaved with it.
- **Bus:** traces are matched by colour, not position; the 6 bus colours are one bus. **The colour that counts is that of the pixel touching the bus:** a colour-2 trace that touches the bus with a colour-9 pixel came out on the colour-9 exit, not the colour-2 one. A bus continues through a cross and through a tunnel pair. Gates of the same ink touching one bus are one gate (AND, XOR, NOT).
- **Mesh:** two colour-2 traces on separate mesh pixels were joined; a colour-9 trace on a third mesh pixel was not. Matching is by colour.
- **Wireless:** a Wireless 0 pixel fed in one place drove a Wireless 0 pixel elsewhere; Wireless 1 stayed off.
- **Clock:** it is **on at tick 0** and flips at every multiple of `clock_interval` (interval 3: on for ticks 0–2, off 3–5, on 6–8; interval 1: on at even ticks). An input on a clock changes nothing.
- **Latch:** a second input that rises later toggles the latch back. An input that falls does nothing, also when another input rises in the same tick.
- **Shared Write pixel:** a Write pixel touching a Latch On and a Latch Off carried the Latch On's output, whichever side each was on.
- **Breakpoint:** a Breakpoint that turns on at tick 7 (its input trace on at tick 6) paused the running game with the counter showing **8**. So the counter at a pause is one more than the tick the Breakpoint turned on. Which tick's state the paused game holds is not yet known (question 17).
- **Latch On with an input** (`latch-on` board): it is on at tick 0 and is toggled off at tick 1 by an input that is on from the start; with an idle input it stays on. Latch On toggles like Latch Off.
- **Read and Write pixels inside a trace** are just trace. An LED can drive another component through a Write pixel.

### Small combinations (2026-11-16, `combinations` board)
A ring oscillator, a NOR SR latch, a NAND D flip-flop, a latch ripple counter and an adder, all laid out on buses with one trace colour per signal. The engine matched all 16 LEDs at every tick 0–15 on the first run, including the ten ticks in which the flip-flop's two outputs flicker together before settling. No new rule was needed.

### Random, recorded through VMem (2026-11-15)
Boards `random-vmem-seed-1` and `-2` (`engine/boards/`): a counter on the VMem address pins steps every 24 ticks, and 32 Random pixels wired to the content pins are triggered once per step. Each board ran to address 128 and was saved after returning to edit mode. The saves are `engine/tests/boards/random-vmem-seed-N.after.vcb`: words 1–127 hold 32 bits each, word 0 is 0.
- **Each trigger is a fresh, fair coin flip.** 50% of the bits are 1, and a bit that was 1 is 1 again in the next word 25% of the time, for both seeds. The two seeds give different words.
- **The output does not hold.** The user saw that a Random's output is only on briefly after it is triggered: a write pulse delayed by 12 buffers came too late to catch it. The manual's "toggles randomly" does not describe a held state. When exactly it turns off is not yet known (question 14).
- **The generator is MT19937** (the C++ `std::mt19937`), seeded with `random_seed`, and each draw's **low bit** is the coin flip. The engine reproduces all 2 × 4064 recorded bits. How it was found: the game's executable (a custom build of Godot 3.5.1 with a native simulator; its method names include `set_random_seed`, `set_clock_timer_intervals`, `solve`, `get_vmem_persistent`) contains the MT19937 constants. The game's scripts are encrypted and were not opened.
- **A Random draws every time it is evaluated**, that is, whenever one of its inputs changes: once when the input rises (that draw is the output) and once when it falls (that draw is used up). So the recorder board consumed 64 draws per cycle. An earlier search that assumed one draw per trigger found nothing for that reason.
- **Draw order within a tick:** on the recorder board the 32 Randoms drew right to left, which is the order their Read pixels are reached when the pulse trace is walked from its first (top-left-most) pixel. The engine walks traces breadth first to get this order; the rule beyond this board is question 22.
- **`random-pulse` board (2026-11-16):** eight Randoms, each on its own trace from a Latch On, seed 1. LEDs 1, 2, 5, 6, 7, 8 lit at tick 2 and stayed lit. So a Random's output **stays on while its input stays on**; Randoms on separate traces drew **left to right** (the first eight low bits of MT19937 with seed 1 are 1, 1, 0, 0, 1, 1, 1, 1); and the Random was on at tick 1, one tick after its input. The engine predicted exactly this.
- **Timer:** on the same board the Timer's LED never lit during ticks 0–6 of stepping. A timer starts off, unlike a clock, and did not flip while the user stepped.
- **This run also confirmed VMem behaviour** with 8 address bits and 32 content bits: a content pin toggled by its input writes the word at the current address; an address change loads the new word (0) into the content pins; an 8-bit counter whose pins all toggle in the same tick steps the address cleanly. The write that went to address 0 does not show in the save.
- **Saving:** both saves landed in `<name>.vcb.tmp` when the user saved over the open file.

## VMem
- **Layout** (verified with `saves/all-inks.vcb.tmp`, filled by hand in the editor): 2^20 words × 32 bits, **big-endian**, so word `i` is bytes `4i..4i+3`. Total 4 MiB. The file always stores the full space; `vmem_settings` only limits which addresses the circuit's pins can reach.
- **External VMem:** with `vmem_is_external: true`, the inline `vmem_data` is an empty blob (trailer `(2, 0)` only). The memory lives in `<project>.vcbmem` next to the `.vcb`; there is no path field. The `.vcbmem` is raw and uncompressed, 4,194,304 bytes, same layout, no trailer. Verified: a hand-written file with word i = `0x112233_0i` showed in the game's VMem editor as `11 22 33 01, 11 22 33 02, …`.
- **Word 0 is special:** the VMem editor always shows word 0 as `0` and won't let you edit it, even when the file holds a non-zero value there. The circuit **can** write word 0 (the editor then showed 9). Whether a preloaded word 0 reaches the circuit is open question 8.
- **`vmem_settings`** = [address bits, x, y, offsetX, offsetY, sizeX, sizeY, content bits, x, y, offsetX, offsetY, sizeX, sizeY, persistent from, persistent to]. The order matches the VMem Settings panel (screenshot `manual/Screenshot 2026-11-15 at 11.32.05.png`: Address Latches and Content Latches each have Bits, Position XY, Offset XY, Size XY; then Persistent Memory From/To, shown as `0x0`). Verified 2026-11-15: setting From `0x0`, To `0xF` changed the last two values to `0, 15`. Bit k sits at `(x − strideX·k, y + strideY·k)`, so bit 0 is the rightmost and the bits run left (verified with `saves/vmem-test2.vcb`). The last two values and data widths under 32 bits are open question 4.
- **At least 4 bits each** (observed 2026-11-16): a file with 2 address bits and 2 content bits came back from the game with both set to 4. The extra pins land at the next offsets, where they can sit on empty board or on top of other pins. The engine refuses fewer than 4 bits.
- **Pins:** each address and data bit is a 1-pixel latch, fed and read through Write → Read pixels, as in the user's CPU. A Latch On pixel used as an address bit did **not** make the address non-zero, so the game probably forces pin latches off at start.
- **Timing** (user-observed, `vmem-test`, `vmem-test2`, `vmem-test3`):
  - A write happens when the data latches change, and goes to the address held at that moment.
  - A data change on the same tick as an address change goes to the **old** address (`vmem-test2`: address and data changed together, so word 0 = 3).
  - When the address changes, memory loads into the data latches.
  - `vmem-test3` (data delayed through 6 buffers): word 1 = 3 and word 0 = 0 while running.
- **Tick-by-tick timing** (observed 2026-11-16, `vmem-timing` board: LEDs on two address and two content pins, stepped for ticks 0–18):
  - When the address pins flip at tick T, the content pins keep their old value during T and take the word at the new address at **tick T+1**. A reader of a content pin sees it at T+2.
  - **Tick T+1 is the locked tick.** The Event Log reported "input to CONTENT latch rejected while the VMem was locked" for a content toggle sent in that tick, and the same for an ADDRESS latch. The rejected toggle is lost, not delayed.
  - A content toggle two or more ticks after the address change is accepted and written.
  - **Address 0 reads back:** the circuit wrote 1 to address 0, moved to address 1 and returned; the content pins showed 1 again. So word 0 is ordinary memory while the simulation runs.
  - Content and address toggled in the same tick: the content pins show the toggled value for one tick, then the word at the new address. This fits "the write goes to the old address first".
  - The engine matches all four LEDs at every tick.
- **`32-bit-alu-try.vcb`:** only a 17-byte test pattern is non-zero (words 1–5). It holds no program.

## Saves and assembly
- **Persistent Memory captures VMem written by the circuit** (verified 2026-11-15, `saves/vmem-preserve-before.vcb` and `saves/vmem-preserve-after.vcb`, made from `vmem-test3`): with Persistent Memory From `0x0` To `0xF`, simulating until word 1 = 3, returning to edit mode and saving gave a `vmem_data` with word 1 = 3. The "before" save, made with the same settings but before simulating, was all zeros. Whether To is inclusive is untested.
- **Without a persistent region, saves never capture runtime state.** Pixels hold no state, and memory written by the circuit isn't saved: `saves/vmem-test3_2.vcb` was saved while running with word 1 = 3 on screen, and its `vmem_data` is all zeros. Saves hold only what is set in the editor.
- External VMem doesn't help (`saves/vmem-ext.vcb`): the game wrote neither the existing `.vcbmem` while running nor a new one on Save As.
- **The game assembles only at simulation start** (a "compiling assembly" loading bar) and never saves the result. A `.vcb` never contains an assembled program.
- **Under Wine (Mac):** overwriting an existing save can leave the new content in `<name>.vcb.tmp`. Saving to a new filename works normally.

## What the User Guide says
The in-game User Guide is transcribed in [`../manual/`](../manual/README.md). These rules come from it. They are the starting model for the engine, but each one still needs an observed fixture before its ink counts as verified.

**Entities and connections** (manual chapters 2–3)
- Pixels connect only to their four direct neighbours.
- Component pixels of the **same** ink that touch are one entity, of any shape. Different component inks that touch stay separate.
- All 16 trace colours plus Read and Write merge into **one** trace entity when they touch. Colour is visual only.
- Components and traces never connect directly. Component → Write → trace → Read → component. The Write or Read pixel must be a direct neighbour of the component. A Read or Write pixel in the middle of a trace is just trace.
- **Connections are not countable:** a component and a trace are either connected or not, however many Read/Write pixels join them. A gate's "inputs" are the distinct traces it reads.
- A trace is on when any component writing to it is on.

**Gates** (A = active inputs, T = total inputs)

| Ink | On when |
|---|---|
| Buffer, OR | A > 0 |
| AND | A > 0 and A == T |
| XOR | A is odd |
| NOT, NOR, NAND, XNOR | the inverse of Buffer, OR, AND, XOR |

**General components**
- **Latch On/Off:** toggles on the rising edge of any input. Latch On starts active.
- **Clock:** toggles every N ticks (`clock_interval`).
- **LED:** on when A > 0. Its colour shows A, up to 15 (`led_palette`).
- **Timer:** toggles every N **real-life milliseconds** (`timer_interval`). It cannot be tick-exact.
- **Random:** toggles randomly on the rising edge of any input. Seed is time-based by default or user-defined (`random_seed`, `random_is_time_seed`). The game is made with Godot, so its generator is the first candidate to try.
- **Breakpoint:** on when A > 0. Fires on the rising edge of its **own state** and pauses the simulation.
- **Wireless 0–3:** each of the four inks is one board-wide OR gate.

**Space optimization** (work with traces and with components; apart from Bus, they don't interact with each other when side by side)
- **Cross:** one pixel at a perpendicular junction; the horizontal and vertical entities pass over each other. Crosses can't chain: they must be at least one pixel apart.
- **Tunnel:** an entity touching a tunnel pixel on one side connects to the first tunnel pixel in that direction that has the **same type of ink** touching its far side. Anything may lie in between, including tunnels for other inks.
- **Mesh:** traces, buses and components touching mesh pixels anywhere on the board merge. The manual's picture shows two differently coloured traces staying separate, so the exact merge rule is open (question 3).
- **Bus:** lets several entities pass through the same cells. The 6 bus colours merge when they touch. How entities are matched across a bus is open (question 3).
- **Annotation, Filler:** no logic.

**VMem** (manual chapter 6)
- The address and content pins "behave just like latches" and are placed by the game when the simulation starts. This matches the observation that a Latch On pin started off.
- **Locking (2-tick access):** VMem is locked in the tick after an address change. Input to its latches while locked is rejected and logged.
- **Content precedence:** if address and content latches toggle in the same tick, the current word is modified first and then the new word is loaded. This matches the observed write-to-old-address behaviour.
- The assembler skips word 0 ("address 0 is reserved and should not be used") and starts at address 1.
- **A memory region can be "preserved at the end of the simulation"** (VMem Settings menu): it is copied into the project's memory. Verified, see Saves.
- The VMem Editor shows the current address and Locked/Ready state during simulation.

**Other**
- The game autosaves every three minutes and before entering simulation mode.
- The blueprint text format ("VCB+" prefix, base64, big-endian, zstd RGBA8 layer blocks) is specified in the manual's appendix.
- `timer_interval` in `saves/timer-and-random.vcb` is 1000, with `random_seed: 1` and `random_is_time_seed: true`.

## Open questions
Owner: the user (in-game checks) and the engine author (research). Numbers are kept from the original spec; 1, 2 and 5 are answered above.

3. **The newer inks** (before the ladder rung that uses them): the manual gives the rules above. What is still unobserved is split into questions 11–21. Timer is answered: real milliseconds.
4. **`vmem_settings`** (before rung C): what a pin Size other than 1×1 does. The last two values are answered (Persistent Memory From and To). Content width under 32 bits is question 21.
6. **First-tick behaviour:** answered 2026-11-15, see "Observed on the `gates` board".
7. **`layers[1]` and `layers[2]`:** presumably decoration. Not needed for v1; the writer emits empty layers.
8. **VMem address 0:** at run time it is ordinary memory: a value written to address 0 reads back (answered 2026-11-16). It is not included in a Persistent Memory save. Still unobserved: whether a non-zero word 0 preloaded in the file reaches the circuit.
9. **In-game TPS** of the 32-bit board on the Windows PC (speed baseline). Needed before the v2 benchmark.
10. **Persistent Memory:** answered 2026-11-15, it works (see Saves). Still open: whether the To address is inclusive, and whether the copy happens only on return to edit mode.

Questions 11–21 come from the v1 design (`vcb-engine-v1.md` lists the interim model and the milestone for each).

11. **Latch, several inputs:** answered 2026-11-15: the latch toggles once per rising input, whatever the other inputs do; falling inputs do nothing.
12. **"Same ink" at cross, tunnel, bus and mesh:** answered 2026-11-15: matching is by the colour of the touching pixel at a tunnel, bus and mesh; a cross joins traces of any colour. Still unobserved: a Read or Write pixel touching a tunnel, bus or mesh (the CPU has none).
13. **Clock:** answered 2026-11-15: on at tick 0, flips at multiples of the interval; inputs are ignored.
14. **Random:** answered 2026-11-16: MT19937 seeded with `random_seed`, low bit, one draw per evaluation; the output stays on while an input stays on.
15. **VMem locking:** answered 2026-11-16: the one tick after an address change; a rejected toggle is lost.
16. **VMem read latency:** answered 2026-11-16: the tick after the address pins flip.
17. **Breakpoint pause:** the counter shows the tick the Breakpoint turned on, plus one (answered 2026-11-15). Still open, before the CPU rung: does the paused game hold the state of that tick or of the next one (does VMem written in the extra tick show)?
18. **Latch On at tick 0:** answered 2026-11-15: its trace is on at tick 0 and a latch reading it sees a rising edge.
19. **Assembly and memory:** does a non-empty `assembly` field overwrite `vmem_data` at simulation start? With Persistent Memory covering the program area, does a save after a run contain the assembled program? If yes, programs can be assembled by the game and no hand-encoding or separate assembler is needed.
20. **Shared Read/Write pixel:** one Read pixel touching two components feeds both (answered 2026-11-15). A shared Write pixel also connects both (observed on `general`).
21. **Content width under 32 bits:** what does a write do to the bits above the content width?
22. **Random draw order:** two cases observed and matched by the engine: Randoms along one trace draw in the order a walk from the trace's first pixel reaches them, and Randoms on separate traces that turn on in the same tick draw in board order (left to right on `random-pulse`). The general rule for mixed cases is not known.
