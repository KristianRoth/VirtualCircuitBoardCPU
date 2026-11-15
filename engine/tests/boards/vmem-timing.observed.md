# Checklist: vmem-timing

- Board: `vmem-timing.vcb` (picture with numbered LEDs: `vmem-timing.png`)
- Board SHA-256: `e7e0b0da887be3817549c837b25726b5d284fff804202a0c1e214b8082b9f089`
- Game version: 1.0.1
- Covers: vmem, 8, 10, 15, 16, 21

Open the board, start the simulation paused, and step one tick at a time.
For each tick below, write the numbers of the LEDs that are lit (or `none`).

## LEDs

| # | Label | Board x,y |
|---|---|---|
| 1 | address_1 | 29,32 |
| 2 | address_0 | 35,32 |
| 3 | content_1 | 53,32 |
| 4 | content_0 | 59,32 |

## Observations

- tick 0, lit LEDs: none
- tick 1, lit LEDs: none
- tick 2, lit LEDs: none
- tick 3, lit LEDs: 4
- tick 4, lit LEDs: 2, 4
- tick 5, lit LEDs: 2, 3
- tick 6, lit LEDs: 2, 3
- tick 7, lit LEDs: 2, 3, 4
- tick 8, lit LEDs: 3, 4
- tick 9, lit LEDs: 4
- tick 10, lit LEDs: 4
- tick 11, lit LEDs: 1, 4
- tick 12, lit LEDs: 1, 3
- tick 13, lit LEDs: 1, 2, 3
- tick 14, lit LEDs: 1, 2, 3, 4
- tick 15, lit LEDs: 1, 2, 3
- tick 16, lit LEDs: 2
- tick 17, lit LEDs: 2, 3, 4
- tick 18, lit LEDs: 2, 3, 4

## Other

- Tick counter when a Breakpoint paused the game (if any): 
- Anything unexpected: 
    - Tick 4 input to CONTENT latch rejected while the VMem was locked. 1 occurance registered at tick 4
    - Tick 8 input to ADDRESS latch rejected while the VMem was locked. 1 occurance registered at tick 8
- Observed by: user, 2026-11-16, macOS (Wine)
