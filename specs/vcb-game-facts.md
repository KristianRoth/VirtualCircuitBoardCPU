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

## VMem
- **Layout** (verified with `saves/all-inks.vcb.tmp`, filled by hand in the editor): 2^20 words × 32 bits, **big-endian**, so word `i` is bytes `4i..4i+3`. Total 4 MiB. The file always stores the full space; `vmem_settings` only limits which addresses the circuit's pins can reach.
- **External VMem:** with `vmem_is_external: true`, the inline `vmem_data` is an empty blob (trailer `(2, 0)` only). The memory lives in `<project>.vcbmem` next to the `.vcb`; there is no path field. The `.vcbmem` is raw and uncompressed, 4,194,304 bytes, same layout, no trailer. Verified: a hand-written file with word i = `0x112233_0i` showed in the game's VMem editor as `11 22 33 01, 11 22 33 02, …`.
- **Word 0 is special:** the VMem editor always shows word 0 as `0` and won't let you edit it, even when the file holds a non-zero value there. The circuit **can** write word 0 (the editor then showed 9). Whether a preloaded word 0 reaches the circuit is open question 8.
- **`vmem_settings`** = [address bits, x, y, offsetX, offsetY, sizeX, sizeY, content bits, x, y, offsetX, offsetY, sizeX, sizeY, persistent from, persistent to]. The order matches the VMem Settings panel (screenshot `manual/Screenshot 2026-11-15 at 11.32.05.png`: Address Latches and Content Latches each have Bits, Position XY, Offset XY, Size XY; then Persistent Memory From/To, shown as `0x0`). Verified 2026-11-15: setting From `0x0`, To `0xF` changed the last two values to `0, 15`. Bit k sits at `(x − strideX·k, y + strideY·k)`, so bit 0 is the rightmost and the bits run left (verified with `saves/vmem-test2.vcb`). The last two values and data widths under 32 bits are open question 4.
- **Pins:** each address and data bit is a 1-pixel latch, fed and read through Write → Read pixels, as in the user's CPU. A Latch On pixel used as an address bit did **not** make the address non-zero, so the game probably forces pin latches off at start.
- **Timing** (user-observed, `vmem-test`, `vmem-test2`, `vmem-test3`):
  - A write happens when the data latches change, and goes to the address held at that moment.
  - A data change on the same tick as an address change goes to the **old** address (`vmem-test2`: address and data changed together, so word 0 = 3).
  - When the address changes, memory loads into the data latches.
  - `vmem-test3` (data delayed through 6 buffers): word 1 = 3 and word 0 = 0 while running.
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
8. **VMem address 0** (before rung C): does the circuit always read address 0 as 0, and are writes to it dropped, or does only the editor hide it? Test board: write a value to address 0, read it back onto LEDs, and also preload a non-zero word 0 through `.vcbmem`.
9. **In-game TPS** of the 32-bit board on the Windows PC (speed baseline). Needed before the v2 benchmark.
10. **Persistent Memory:** answered 2026-11-15, it works (see Saves). Still open: whether the To address is inclusive, and whether the copy happens only on return to edit mode.

Questions 11–21 come from the v1 design (`vcb-engine-v1.md` lists the interim model and the milestone for each).

11. **Latch, several inputs:** two inputs rising in the same tick: answered 2026-11-15, the latch toggles twice (no change). Still open: one input rises while another is already on; one falls as another rises.
12. **"Same ink" at cross, tunnel, bus and mesh:** is matching by the colour of the touching trace pixel? Does a cross join traces of different colours? What does a Read or Write pixel touching a tunnel, bus or mesh match? The user is sure that at a bus the same ink continues.
13. **Clock:** on which tick does a clock first flip, for intervals 1, 2 and 3? Does a clock with an input behave differently?
14. **Random:** the toggle probability, whether there is one draw per component or per rising input, and which generator (the game is made with Godot).
15. **VMem locking:** exactly which ticks reject input, and is a rejected toggle lost or delayed?
16. **VMem read latency:** on which tick after an address pin flips do the content pins show the new word?
17. **Breakpoint pause:** which counter value does the game show at the pause, relative to the tick the Breakpoint turned on?
18. **Latch On at tick 0:** answered 2026-11-15: its trace is on at tick 0 and a latch reading it sees a rising edge.
19. **Assembly and memory:** does a non-empty `assembly` field overwrite `vmem_data` at simulation start? With Persistent Memory covering the program area, does a save after a run contain the assembled program? If yes, programs can be assembled by the game and no hand-encoding or separate assembler is needed.
20. **Shared Read/Write pixel:** one Read pixel touching two components feeds both (answered 2026-11-15). A shared Write pixel is still unobserved.
21. **Content width under 32 bits:** what does a write do to the bits above the content width?
