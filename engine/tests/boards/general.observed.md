# Checklist: general

- Board: `general.vcb` (picture with numbered LEDs: `general.png`)
- Board SHA-256: `2c4632e2af6c578ce899a6b351795fe60aa149415bbfb625aa327e7afced34c1`
- Game version: 1.0.1
- Covers: Clock, Breakpoint, Latch, 11, 13, 17, 20

Open the board, start the simulation paused, and step one tick at a time.
For each tick below, write the numbers of the LEDs that are lit (or `none`).

## LEDs

| # | Label | Board x,y |
|---|---|---|
| 1 | clock | 17,45 |
| 2 | clock_with_input | 22,45 |
| 3 | latch_second_rise | 27,45 |
| 4 | latch_fall_and_rise | 32,45 |
| 5 | shared_write_a | 37,45 |
| 6 | shared_write_b | 42,45 |
| 7 | breakpoint_chain | 47,45 |

## Observations

- tick 0, lit LEDs: none
- tick 1, lit LEDs: 1, 2, 5, 6
- tick 2, lit LEDs: 1, 2, 3, 4, 5, 6
- tick 3, lit LEDs: 1, 2, 3, 5, 6
- tick 4, lit LEDs: 5, 6
- tick 5, lit LEDs: 5, 6
- tick 6, lit LEDs: 5, 6
- tick 7, lit LEDs: 1, 2, 5, 6, 7
- tick 8, lit LEDs: 1, 2, 5, 6, 7
- tick 9, lit LEDs: 1, 2, 5, 6, 7
- tick 10, lit LEDs: 5, 6, 7

## Other

- Tick counter when a Breakpoint paused the game (if any): 8
- Anything unexpected: 
- Observed by: user, 2026-11-15, macOS (Wine)
