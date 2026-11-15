# Checklist: random-pulse

- Board: `random-pulse.vcb` (picture with numbered LEDs: `random-pulse.png`)
- Board SHA-256: `e2e18ba243b1429d1b04dd416842a42dc063c91de9295f22ad76725b19c7c520`
- Game version: 1.0.1
- Covers: Random, Timer, 14

Open the board, start the simulation paused, and step one tick at a time.
For each tick below, write the numbers of the LEDs that are lit (or `none`).

## LEDs

| # | Label | Board x,y |
|---|---|---|
| 1 | random_1 | 17,25 |
| 2 | random_2 | 22,25 |
| 3 | random_3 | 27,25 |
| 4 | random_4 | 32,25 |
| 5 | random_5 | 37,25 |
| 6 | random_6 | 42,25 |
| 7 | random_7 | 47,25 |
| 8 | random_8 | 52,25 |
| 9 | timer | 57,25 |

## Observations

- tick 0, lit LEDs: none
- tick 1, lit LEDs: none
- tick 2, lit LEDs: 1, 2, 5, 6, 7, 8
- tick 3, lit LEDs: 1, 2, 5, 6, 7, 8
- tick 4, lit LEDs: 1, 2, 5, 6, 7, 8
- tick 5, lit LEDs: 1, 2, 5, 6, 7, 8
- tick 6, lit LEDs: 1, 2, 5, 6, 7, 8

## Other

- Tick counter when a Breakpoint paused the game (if any): 
- Anything unexpected: 
- Observed by: user, 2026-11-16, macOS (Wine)
- timer ticks: 1000
- LED 9 (Timer) was never lit in ticks 0-6, so a timer starts off and did not flip while the user stepped.
  The engine run uses a timer interval longer than the observation.
