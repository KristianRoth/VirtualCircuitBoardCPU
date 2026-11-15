# Checklist: latch-on

- Board: `latch-on.vcb` (picture with numbered LEDs: `latch-on.png`)
- Board SHA-256: `883af1aa09064676555b70531b6f9a81ab396635cfe714731e675077a2ee44b6`
- Game version: 1.0.1
- Covers: Latch On

Open the board, start the simulation paused, and step one tick at a time.
For each tick below, write the numbers of the LEDs that are lit (or `none`).

## LEDs

| # | Label | Board x,y |
|---|---|---|
| 1 | latch_on_with_rising_input | 17,25 |
| 2 | latch_on_with_idle_input | 22,25 |

## Observations

- tick 0, lit LEDs: none
- tick 1, lit LEDs: 1, 2
- tick 2, lit LEDs: 2
- tick 3, lit LEDs: 2

## Other

- Tick counter when a Breakpoint paused the game (if any): 
- Anything unexpected: 
- Observed by: user, 2026-11-15, macOS (Wine)
