#!/usr/bin/env python3
"""Where a run spends its time, from the output of macOS's `sample`: per thread, the samples whose top of
stack is in each function. Usage: python3 tools/sample_summary.py <sample output file> [more files]

How the profile in specs/ is made (from engine/, see the README for PATH and SDKROOT):
    RUSTFLAGS="--cfg separate -C target-cpu=native" cargo build --release --target-dir target/separate
    target/separate/release/vcb run <board> --asm <program> --ticks <n> & sample $! 6 1 -file out.txt
`--cfg separate` keeps the engine's main functions out of each other (no counters, unlike `--cfg profiling`)."""
import re, sys, collections

NAMES = [("Engine9propagate", "propagate"), ("Engine15propagate_split", "propagate_split"), ("Engine9step_real", "step_real"),
         ("Engine7advance", "advance"), ("10Simulation", "advance (replay, idle jumps, the loop over ticks)"), ("Engine6intern", "Engine::intern"), ("Engine7restore", "restore"), ("Engine10begin_step", "begin_step"),
         ("Engine11leave_split", "leave_split"), ("Engine9vmem_tick", "vmem_tick"), ("Engine10proof_tick", "proof_tick"), ("Engine11start_proof", "start_proof"),
         ("Engine11make_record", "make_record"), ("Engine7install", "install"), ("Engine3ask", "the worker's loop (prove)"), ("Table6intern", "Table::intern"),
         ("Table4node", "Table::node"), ("Table6record", "Table::record"), ("Table7arrival", "Table::arrival"), ("Table4edge", "Table::edge"),
         ("Run8simulate", "the run loop (Run::simulate)"), ("memmove", "memmove"), ("memcmp", "memcmp"), ("semaphore_wait", "waiting"),
         ("__psynch", "waiting"), ("reserve_rehash", "hash map growth"), ("grow_one", "list growth"), ("malloc", "allocator"), ("xzm", "allocator"),
         ("free", "allocator"), ("realloc", "allocator"), ("bzero", "memset"), ("memset", "memset")]

def name(symbol):
    for key, value in NAMES:
        if key in symbol:
            return value
    return symbol[:60]

for path in sys.argv[1:]:
    threads, stack, current = collections.OrderedDict(), [], None
    for line in open(path):
        if line.startswith("Total number") or line.startswith("Sort by"):
            break
        m = re.match(r"^([ +!:|]*?)(\d+) (.*)$", line.rstrip("\n"))
        if not m:
            continue
        depth, count, rest = len(m.group(1)), int(m.group(2)), m.group(3)
        if rest.startswith("Thread_"):
            current = threads.setdefault(rest.split("   ")[0] + (" (main)" if "main-thread" in rest else ""), collections.Counter())
            stack = []
        if current is None:
            continue
        while stack and stack[-1][0] >= depth:
            stack.pop()
        if stack:
            current[stack[-1][1]] -= count  # a child's samples are not the parent's own
        symbol = name(rest.split("  (in ")[0])
        current[symbol] += count
        stack.append((depth, symbol))
    print(path)
    for thread, own in threads.items():
        total = sum(v for v in own.values() if v > 0)
        busy = total - own.get("waiting", 0)
        print(f"  {thread}: {total} samples, {busy} not waiting")
        for symbol, count in own.most_common():
            if count * 200 >= max(busy, 1) and symbol != "waiting" and not symbol.startswith("Thread_"):
                print(f"    {100 * count / max(busy, 1):5.1f}%  {count:6d}  {symbol}")
