# Checklist: combinations

- Board: `combinations.vcb` (picture with numbered LEDs: `combinations.png`)
- Board SHA-256: `ea179a56f2b1bb030fe3428f3afa6730a9240271fdd5bada3144e034d9f6555f`
- Game version: 1.0.1
- Covers: rungB

Open the board, start the simulation paused, and step one tick at a time.
For each tick below, write the numbers of the LEDs that are lit (or `none`).

## LEDs

| # | Label | Board x,y |
|---|---|---|
| 1 | ring | 17,28 |
| 2 | sr_q | 31,28 |
| 3 | sr_qn | 36,28 |
| 4 | dff_clock | 77,28 |
| 5 | dff_d | 82,28 |
| 6 | dff_q | 87,28 |
| 7 | dff_qn | 92,28 |
| 8 | count_0 | 141,28 |
| 9 | count_1 | 146,28 |
| 10 | count_2 | 151,28 |
| 11 | count_3 | 156,28 |
| 12 | sum_0 | 161,28 |
| 13 | sum_1 | 166,28 |
| 14 | sum_2 | 171,28 |
| 15 | sum_3 | 176,28 |
| 16 | carry_out | 181,28 |

## Observations

- tick 0, lit LEDs: none
- tick 1, lit LEDs: 2, 3, 4, 6, 7
- tick 2, lit LEDs: 4, 5, 8, 9, 10, 11, 12, 13
- tick 3, lit LEDs: 1, 2, 4, 5, 6, 7, 8, 9, 10, 11, 14, 15
- tick 4, lit LEDs: 1, 2, 5, 8, 9, 10, 11, 13, 14, 15
- tick 5, lit LEDs: 1, 2, 5, 6, 7, 8, 9, 10, 11, 13, 15
- tick 6, lit LEDs: 2, 5, 8, 9, 10, 11, 13, 16
- tick 7, lit LEDs: 4, 5, 6, 7, 8, 9, 10, 11, 13, 16
- tick 8, lit LEDs: 3, 4, 9, 10, 11, 13, 16
- tick 9, lit LEDs: 1, 3, 4, 6, 7, 9, 10, 11, 12, 13, 16
- tick 10, lit LEDs: 1, 3, 6, 10, 11, 12, 16
- tick 11, lit LEDs: 1, 3, 6, 10, 11, 12, 13, 16
- tick 12, lit LEDs: 3, 6, 11, 12, 13, 16
- tick 13, lit LEDs: 3, 4, 6, 11, 12, 13, 16
- tick 14, lit LEDs: 3, 4, 5, 6, 8, 12, 13, 15
- tick 15, lit LEDs: 1, 3, 4, 5, 6, 7, 8, 13

## Other

- Tick counter when a Breakpoint paused the game (if any): 
- Anything unexpected: 
- Observed by: user, 2026-11-16, macOS (Wine)
