# Checklist: gates-recheck

- Board: `gates-recheck.vcb` (picture with numbered LEDs: `gates-recheck.png`)
- Board SHA-256: `7a711fc50aafa5b40d0b8f78485ba2cc1cdfbc84dfcb582f9f10909e6b308b37`
- Game version: 1.0.1
- Covers: AND, NAND

Open the board, start the simulation paused, and step one tick at a time.
For each tick below, write the numbers of the LEDs that are lit (or `none`).

## LEDs

| # | Label | Board x,y |
|---|---|---|
| 1 | and_10 | 17,25 |
| 2 | and_01 | 22,25 |
| 3 | and_11 | 27,25 |
| 4 | nand_10 | 32,25 |
| 5 | and_no_input | 37,25 |
| 6 | nand_no_input | 42,25 |
| 7 | not_no_input | 47,25 |

## Observations

- tick 0, lit LEDs: none
- tick 1, lit LEDs: 4, 7
- tick 2, lit LEDs: 3, 4, 7
- tick 3, lit LEDs: 3, 4, 7

## Other

- Tick counter when a Breakpoint paused the game (if any): No
- Anything unexpected: No
- Observed by: user, 2026-11-15, macOS (Wine)
