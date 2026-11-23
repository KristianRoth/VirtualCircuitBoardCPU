# VirtualCircuitBoardCPU

**`vcb`, an engine that runs saves of the Steam game [Virtual Circuit Board](https://store.steampowered.com/app/1885690/) outside the game**, tick for tick like the game does, and much faster. It reads a `.vcb` file, simulates the drawn circuit, and stops where the game would: on a Breakpoint, at a memory address, or after a number of ticks. It also has the game's assembler, so a board with a program in its Assembly Editor runs as it is.

The repository started as a 32-bit CPU drawn in the game. The CPU is now the engine's largest test board, and it lives with its programs in a repository of its own.

- **Exact:** every ink and the virtual memory are checked against saves made by the game. The CPU runs Advent of Code 2025 day 1 part 1 to its Breakpoint at the game's tick with identical memory.
- **Fast:** day 4 part 2 of Advent of Code 2025 on the CPU is 1,339 million ticks. The engine runs it in about 6.4 seconds; the game, at about 5 million ticks a second, would need about 268.

## Where to read on

| | |
|---|---|
| [`engine/README.md`](engine/README.md) | The engine: building, every command and option, running a program on the CPU, how the code is laid out, how correctness is established, speed. |
| [`specs/vcb-game-facts.md`](specs/vcb-game-facts.md) | Everything observed about the game and the `.vcb` format, and the questions still open. |
| [`specs/vcb-engine.md`](specs/vcb-engine.md), [`specs/vcb-engine-v1.md`](specs/vcb-engine-v1.md) | What the engine must do, and how the reference engine (v1) works. |
| [`specs/vcb-engine-v2.md`](specs/vcb-engine-v2.md), [`specs/vcb-engine-v2.1-end.md`](specs/vcb-engine-v2.1-end.md) | The fast engine (v2): its design, where its speed stands, what was tried and what is left. The second file is the place to start. |
| [`specs/vcb-assembler.md`](specs/vcb-assembler.md) | The assembler: the game's assembly language as the game was seen to treat it. |
| [`vscode-vcbasm/README.md`](vscode-vcbasm/README.md) | A VS Code extension that colours `.vcbasm` assembly files. |
| [AoC2025](https://github.com/KristianRoth/AoC2025#readme) | The CPU's boards, its instruction set, and all of Advent of Code 2025 solved on it, with the run time of every solution. |
| [AoC2026](https://github.com/KristianRoth/AoC2026#readme) | Advent of Code 2026 on the same CPU. |

## Getting started

You do not need the game to run the engine. You need git and Rust.

**1. Install Rust** with [rustup](https://rustup.rs). The engine pins its Rust version in `engine/rust-toolchain.toml`; rustup fetches that version by itself the first time you build.

**2. Get the code and build it.**

```
git clone https://github.com/KristianRoth/VirtualCircuitBoardCPU.git
cd VirtualCircuitBoardCPU/engine
cargo build --release
```

The program is then `engine/target/release/vcb`. The release build is made for the processor of the machine it is built on, so build it on the machine that will run it.

**3. Check that it works.** This runs the CPU, with Advent of Code 2025 day 1 part 1 already in its memory, to its Breakpoint:

```
./target/release/vcb run tests/boards/cpu-aoc1p1.vcb --ticks 100000000 --watch 0x100 --stats
```

It should end like this (the time will differ):

```
stopped at tick 43146512: breakpoint
0x00000100 = 0x00000407 (1031)
```

**4. Run the tests,** if you want to change the engine:

```
cargo test                         # about 30 seconds
cargo test --release -- --ignored  # the CPU run against the game's save
```

**5. Run your own board.** Save it in the game and give the file to the engine:

```
./target/release/vcb info my-board.vcb
./target/release/vcb run my-board.vcb --ticks 1000000 --stats
```

`vcb info` tells you whether the board uses anything the engine has not been checked against. [`engine/README.md`](engine/README.md) has every option: stopping at an address, watching memory, probing a pixel, pictures of the board, another program for the board.

**6. Run an Advent of Code solution on the CPU.** The programs are in their own repository; clone it into this folder (git ignores it here) and name a program with `--asm`:

```
cd ..
git clone https://github.com/KristianRoth/AoC2025.git
cd engine
./target/release/vcb run ../AoC2025/cpu-32bit-autorun-aoc1p1.vcb --asm ../AoC2025/days/day5/part1.vcbasm --ticks 2000000000 --watch 0x100
```

Two things that can go wrong:

- **Linking fails on a very new macOS.** Point the build at an older SDK, as [`engine/README.md`](engine/README.md) shows under "Build and test".
- **You want the last tenth of speed.** `sh tools/build-pgo.sh` in `engine/` makes a profile-guided build; it is described in the same place.

## The CPU

https://github.com/user-attachments/assets/4178198c-dcfb-4dda-80ad-a7bf4f559366

A 32-bit CPU drawn in the game: two 32-bit registers, memory of 16-bit words, a stack, and a small set of instructions without multiply or divide. Its boards, the full instruction set and the programs written for it are in [AoC2025](https://github.com/KristianRoth/AoC2025#readme). A copy of the board is among the engine's tests (`engine/tests/boards/cpu-32bit-manual-start.vcb`), so the engine does not depend on that repository.

## Licence

[MIT](LICENSE). Virtual Circuit Board itself, and the text of its User Guide, belong to the game's makers and are not part of this repository.
