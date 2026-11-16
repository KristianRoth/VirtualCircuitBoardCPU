# VirtualCircuitBoardCPU

A 32-bit CPU built in the Steam game [Virtual Circuit Board](https://store.steampowered.com/app/1885690/), and an engine that runs the game's `.vcb` saves outside the game, tick for tick like the game does.

- [`engine/`](engine/README.md): the `vcb` engine (Rust). It runs the CPU through Advent of Code 2025 day 1 part 1 in about 8 seconds, stopping on the same tick as the game with identical memory.
- [`specs/`](specs/): how the engine was designed and verified, and everything observed about the game and its file format.
- The CPU boards and the Advent of Code programs are in their own repositories: [AoC2025](https://github.com/KristianRoth/AoC2025) and [AoC2026](https://github.com/KristianRoth/AoC2026).

## The CPU

https://github.com/user-attachments/assets/4178198c-dcfb-4dda-80ad-a7bf4f559366

### Instructions
1. No-Op:     0000 xxxx xxxx xxxx
2.
3. Load:      0010 AaBbPB  AaBbPB
4. LoadIA:    0011 OOOO OOOO OOOO
5. LoadIB:    0100 OOOO OOOO OOOO
6. Store:     0101 AaBbPB  AaBbPB
7. Op:        0110 xSSS AOXN OOOO
8. 
9.
10.
11. Push:     1010 AaBb PBxx xxxx
12. Pop:      1011 AaBb PBxx xxxx
13. Jump:     1100 zcnq nxxx xxxx - AAAA AAAA AAAA AAAA
14. 
15. Call:     1110 xxxx xxxx xxxx - AAAA AAAA AAAA AAAA
16. Ret:      1111 xxxx xxxx xxxx

