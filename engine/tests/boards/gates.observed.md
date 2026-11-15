# Checklist: gates

- Board: `gates.vcb` (picture with numbered LEDs: `gates.png`)
- Board SHA-256: `cff41af2c87f6316914b46896b2415d9d31f552fa574d195af0160b97bdbf3d0`
- Game version: 1.0.1
- Covers: rule1, rule2, rule8, rule9, Buffer, AND, OR, XOR, NOT, NAND, NOR, XNOR, Latch, LED, 6, 11, 18, 20

Open the board, start the simulation paused, and step one tick at a time.
For each tick below, write the numbers of the LEDs that are lit (or `none`).

## LEDs

| # | Label | Board x,y |
|---|---|---|
| 1 | latch_on | 17,25 |
| 2 | buffer | 22,25 |
| 3 | not_no_input | 27,25 |
| 4 | two_colours | 32,25 |
| 5 | no_write | 37,25 |
| 6 | buffer_10 | 42,25 |
| 7 | and_10 | 47,25 |
| 8 | or_10 | 52,25 |
| 9 | xor_10 | 57,25 |
| 10 | not_10 | 62,25 |
| 11 | nand_10 | 67,25 |
| 12 | nor_10 | 72,25 |
| 13 | xnor_10 | 77,25 |
| 14 | and_11 | 82,25 |
| 15 | xor_11 | 87,25 |
| 16 | xnor_11 | 92,25 |
| 17 | and_no_input | 97,25 |
| 18 | nand_no_input | 102,25 |
| 19 | xor_two_reads | 107,25 |
| 20 | latch_from_not | 112,25 |
| 21 | latch_two_rises | 117,25 |
| 22 | latch_from_latch_on | 122,25 |
| 23 | shared_read_left | 127,25 |
| 24 | shared_read_right | 131,25 |

## Observations

- tick 0, lit LEDs: none
- tick 1, lit LEDs: 1, 3, 4, 10, 11, 12, 13, 16, 23, 24
- tick 2, lit LEDs: 1, 2, 3, 4, 6, 7, 8, 9, 11, 14, 16, 19, 20, 22, 23, 24
- tick 3, lit LEDs: same
- tick 4, lit LEDs: same
- tick 5, lit LEDs: same
- tick 6, lit LEDs: same

## Other

- Tick counter when a Breakpoint paused the game (if any): No
- Anything unexpected: No
- Observed by: user, 2026-11-15, macOS (Wine)
- ignore LEDs: 7
- Correction: LED 7 (AND with one input on, one off) was listed as lit from tick 2. It was re-observed off on `gates-recheck` (LEDs 1 and 2) the same day, so the fixture test skips LED 7 here.
