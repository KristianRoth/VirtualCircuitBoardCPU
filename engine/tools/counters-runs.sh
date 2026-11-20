#!/bin/sh
# Every run of examples/counters.rs on day 4 part 2 (300 million ticks), with one command:
#   1 to 4: the usual events, a reading every 1,000 steps: the usual engine, without records, with a small replay table, and both;
#   5 and 6: the branch events, a reading every 1,000 steps: the usual engine, and without records;
#   7: the usual events per part of a step (a reading at every boundary inside a step), which needs the build with --cfg parts.
# Needs root, after both builds (from engine/):
#   cargo build --release --example counters
#   RUSTFLAGS="--cfg parts -C target-cpu=native" cargo build --release --example counters --target-dir target/parts
#   sudo sh tools/counters-runs.sh [counters binary] [counters binary built with --cfg parts]
# Without the arguments the binaries are the two built above. BOARD and PROGRAM in the environment replace day 4 part 2,
# and EXTRA is added to every run (EXTRA=dry tries all of it without root: see the example's header).
abs() { case "$1" in /*) echo "$1" ;; *) echo "$PWD/$1" ;; esac; }
[ -n "$1" ] && PLAIN=$(abs "$1")
[ -n "$2" ] && PARTS=$(abs "$2")
cd "$(dirname "$0")/.." || exit 1
PLAIN=${PLAIN:-target/release/examples/counters}; PARTS=${PARTS:-target/parts/release/examples/counters}
B=${BOARD:-../AoC2025/cpu-32bit-autorun-aoc1p1.vcb}; P=${PROGRAM:-../AoC2025/days/day4/part2.vcbasm}
[ -x "$PLAIN" ] || { echo "$PLAIN: not there; build it with cargo build --release --example counters"; exit 1; }
for words in "" "nosplit" "table=8" "nosplit table=8"; do
  echo "=== usual events, a reading every 1,000 steps: ${words:-as usual}"
  "$PLAIN" $B $P 300000000 1000 $words $EXTRA 2>&1 | grep -v "^processor\|reading of the counters"
done
for words in "" "nosplit"; do
  echo
  echo "=== branch events, a reading every 1,000 steps: ${words:-as usual}"
  "$PLAIN" $B $P 300000000 1000 events=branches $words $EXTRA 2>&1 | grep -v "^processor"
done
echo
echo "=== usual events per part of a step, a reading at every boundary"
if [ -x "$PARTS" ]; then
  "$PARTS" $B $P 300000000 1 parts $EXTRA 2>&1 | grep -v "^processor"
else
  echo "$PARTS: not there; build it with RUSTFLAGS=\"--cfg parts -C target-cpu=native\" cargo build --release --example counters --target-dir target/parts"
fi
