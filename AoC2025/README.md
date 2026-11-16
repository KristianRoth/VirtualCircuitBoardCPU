# Advent of Code 2025 on the 32-bit CPU

The CPU's board files for the Steam game Virtual Circuit Board (Update 1.0.1), and the programs written for it.

| File | What it is |
|---|---|
| `cpu-32bit-manual-start.vcb` | The CPU as built. Its run switch (a 5×5 latch block near x 1100, y 249) is Latch Off, so it starts when the latch is clicked in the game. Formerly `32-bit-alu-try.vcb`. |
| `cpu-32bit-autorun-aoc1p1.vcb` | The same CPU with the run switch drawn as Latch On, so it runs by itself from the first tick. Its assembly is `days/day1/part1.vcbasm` (Advent of Code day 1, part 1), and the program ends on a Breakpoint. Formerly `CPU.vcb`. |

The two boards differ only in those 24 latch pixels and in the assembly text they carry.

The engine's test fixture `engine/tests/boards/cpu-aoc1p1.vcb` is the autorun board with the program already assembled into its memory by the game; see `engine/README.md` for how to run it.

## Programs

`days/` holds the assembly programs for the CPU, one folder per day, in the game's assembly language:

| File | Program |
|---|---|
| `days/day1/part1.vcbasm` | Advent of Code day 1, part 1 |
| `days/day1/part2.vcbasm` | Advent of Code day 1, part 2 |
