# Checklist: rvx1

- Board: `rvx1.vcb` (picture with numbered LEDs: `rvx1.png`)
- Board SHA-256: `cf6ecdfbe976de8939972e4fcbb194eb32203242e780615d2ba0737a29597a75`
- Game version: 1.0.1
- Covers: Random, 14

Open the board, start the simulation paused, and step one tick at a time.
For each tick below, write the numbers of the LEDs that are lit (or `none`).

## LEDs

| # | Label | Board x,y |
|---|---|---|

## Observations


## Other

- Tick counter when a Breakpoint paused the game (if any): 
- Anything unexpected: 
- Observed by: user, 2026-11-15, macOS (Wine)
- This board has no LEDs. The user ran it until the Breakpoint paused it, returned to edit mode and saved;
  the game wrote the save as `.vcb.tmp`. It is stored as `random-vmem-seed-1.after.vcb`: words 1-127 hold
  32 Random bits each, word 0 is 0.
