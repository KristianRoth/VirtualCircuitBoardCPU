# Checklist: routing-b

- Board: `routing-b.vcb` (picture with numbered LEDs: `routing-b.png`)
- Board SHA-256: `7e7704bb5f57e055feb14252ad9f5779cd82cc5125d496b361795ab101aa3899`
- Game version: 1.0.1
- Covers: rule3, rule6, rule7, Bus, Mesh, Wireless, 12

Open the board, start the simulation paused, and step one tick at a time.
For each tick below, write the numbers of the LEDs that are lit (or `none`).

## LEDs

| # | Label | Board x,y |
|---|---|---|
| 1 | bus_nine_off | 21,24 |
| 2 | bus_two_on | 25,24 |
| 3 | bus_touching_colour_differs | 34,24 |
| 4 | bus_touching_colour_same | 38,24 |
| 5 | bus_through_cross | 47,24 |
| 6 | bus_through_tunnel | 58,24 |
| 7 | and_through_bus | 63,24 |
| 8 | xor_through_bus | 71,24 |
| 9 | not_through_bus | 79,24 |
| 10 | mesh_same_colour | 92,24 |
| 11 | mesh_other_colour | 96,24 |
| 12 | wireless_0_on | 103,24 |
| 13 | wireless_1_off | 107,24 |

## Observations

- tick 0, lit LEDs: none
- tick 1, lit LEDs: 2, 4, 5, 6, 9, 10
- tick 2, lit LEDs: 2, 4, 5, 6, 7, 8, 10, 12
- tick 3, lit LEDs: same
- tick 4, lit LEDs: same

## Other

- Tick counter when a Breakpoint paused the game (if any): 
- Anything unexpected: 
- Observed by: user, 2026-11-15, macOS (Wine)
