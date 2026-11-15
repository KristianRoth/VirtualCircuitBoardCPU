# Checklist: routing-a

- Board: `routing-a.vcb` (picture with numbered LEDs: `routing-a.png`)
- Board SHA-256: `4746ab6351e8cd871e7a93a3371722b49c9a312bb1ce2e158d45a2809bd78fac`
- Game version: 1.0.1
- Covers: rule4, rule5, rule8, Cross, Tunnel, 12

Open the board, start the simulation paused, and step one tick at a time.
For each tick below, write the numbers of the LEDs that are lit (or `none`).

## LEDs

| # | Label | Board x,y |
|---|---|---|
| 1 | cross_vertical_off | 20,25 |
| 2 | cross_horizontal_on | 24,25 |
| 3 | cross_two_colours | 32,25 |
| 4 | cross_write_read_on | 39,25 |
| 5 | cross_other_off | 43,25 |
| 6 | or_through_cross | 48,25 |
| 7 | xor_through_cross | 57,25 |
| 8 | xnor_through_cross | 66,25 |
| 9 | and_through_cross | 75,25 |
| 10 | tunnel_red_on | 94,25 |
| 11 | tunnel_yellow_off | 98,25 |
| 12 | read_write_inside_trace | 103,25 |
| 13 | led_drives_led | 108,25 |

## Observations

- tick 0, lit LEDs: none
- tick 1, lit LEDs: 2, 3, 4, 8, 10, 12
- tick 2, lit LEDs: 2, 3, 4, 6, 7, 9, 10, 12, 13
- tick 3, lit LEDs: same
- tick 4, lit LEDs: same

## Other

- Tick counter when a Breakpoint paused the game (if any): 
- Anything unexpected: 
- Observed by: user, 2026-11-15, macOS (Wine)
