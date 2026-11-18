#!/usr/bin/env python3
"""The v2 benchmark (specs/vcb-engine-v2.md, "The acceptance rule"). Run from engine/:

    python3 tools/bench.py --old <vcb binary> --new <vcb binary> [--old-engine v1] [--new-engine v1]
                           [--runs 5] [--ticks 100000000] [--full]

Runs every program of the benchmark set alternately on the two builds (old, new, old, new, ...), each to its
Breakpoint or to --ticks, whichever comes first (--full: to the Breakpoint). Per program the best of the runs
counts. The last line is the score: the geometric mean of new speed / old speed. Exits with an error when a
program or board is missing, or when the two builds stop differently.
"""
import argparse, math, os, re, subprocess, sys, time

BOARD = "../AoC2025/cpu-32bit-autorun-aoc1p1.vcb"
# (board, program or None for the board's own, has a Breakpoint, extra options)
PROGRAMS = [(BOARD, f"../AoC2025/days/day{d}/part{p}.vcbasm", True, []) for d in (1, 2, 3, 4) for p in (1, 2)]
# The game's two 32-bit sample computers with their program edited to run the primes program without input. They are
# the game's own boards, so they are not in the repository (saves/ is git-ignored). They never stop: always run to
# --ticks. The compact one has Timer ink, which does not reach its memory (the same writes with 1000 and 77777).
PROGRAMS.append(("../saves/bench/32-bit-cpu-primes.vcb", None, False, []))
PROGRAMS.append(("../saves/bench/32-bit-cpu-compact-primes.vcb", None, False, ["--timer-ticks", "1000"]))


def run(binary, engine, board, program, ticks, options):
    started = time.time()
    out = subprocess.run([binary, "run", board, *(["--asm", program] if program else []), "--ticks", str(ticks), "--engine", engine, "--stats", "--watch", "0x100", *options],
                         capture_output=True, text=True)
    wall = time.time() - started
    if out.returncode != 0:
        sys.exit(f"{binary} failed on {program or board}: {out.stderr.strip()}")
    stats = re.search(r"ticks: (\d+)\s+time: ([\d.]+) s", out.stderr)
    stop = [line for line in out.stdout.splitlines() if not line.startswith("assembled")]
    done, seconds = int(stats.group(1)), float(stats.group(2))
    return done / seconds, wall - seconds, done, "\n".join(stop)


def main():
    a = argparse.ArgumentParser()
    a.add_argument("--old", required=True)
    a.add_argument("--new", required=True)
    a.add_argument("--old-engine", default="v1")
    a.add_argument("--new-engine", default="v1")
    a.add_argument("--runs", type=int, default=5)
    a.add_argument("--ticks", type=int, default=100_000_000)
    a.add_argument("--full", action="store_true")
    args = a.parse_args()
    missing = [f for board, program, _, _ in PROGRAMS for f in (board, program) if f and not os.path.exists(f)]
    if missing:
        sys.exit("missing: " + ", ".join(missing))
    ticks = 10**12 if args.full else args.ticks
    commit = subprocess.run(["git", "-C", "../AoC2025", "rev-parse", "--short", "HEAD"], capture_output=True, text=True).stdout.strip()
    print(f"AoC2025 commit {commit}; {args.runs} runs each, alternating; to the Breakpoint" + ("" if args.full else f" or {ticks:,} ticks"))
    print(f"{'program':<24}{'ticks':>14}{'old M/s':>9}{'new M/s':>9}{'ratio':>8}{'spread old':>11}{'spread new':>11}{'start-up':>10}  stop")
    ratios = []
    for board, program, stops_itself, options in PROGRAMS:
        old, new, start_up, stops = [], [], [], set()
        for _ in range(args.runs):
            for speeds, binary, engine in ((old, args.old, args.old_engine), (new, args.new, args.new_engine)):
                tps, start, done, stop = run(binary, engine, board, program, ticks if stops_itself else args.ticks, options)
                speeds.append(tps)
                start_up.append(start)
                stops.add((done, stop))
        ratio = max(new) / max(old)
        ratios.append(ratio)
        spread = lambda s: f"{100 * (max(s) - sorted(s)[len(s) // 2]) / max(s):.1f}%"
        name = "/".join(program.split("/")[-2:]).replace(".vcbasm", "") if program else board.split("/")[-1].replace("32-bit-cpu-", "sample-").replace(".vcb", "")
        done = sorted(stops)[0][0]
        print(f"{name:<24}{done:>14,}{max(old) / 1e6:>9.2f}{max(new) / 1e6:>9.2f}{ratio:>8.3f}{spread(old):>11}{spread(new):>11}{min(start_up):>9.2f}s  {'equal' if len(stops) == 1 else 'DIFFERENT'}", flush=True)
        if len(stops) != 1:
            sys.exit(f"the two builds stop differently on {name}")
    score = math.exp(sum(math.log(r) for r in ratios) / len(ratios))
    print(f"score (geometric mean of new / old): {score:.4f}")


main()
