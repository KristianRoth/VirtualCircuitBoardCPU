#!/bin/sh
# Builds target/release/vcb with profile-guided optimisation (v2 attempt 24, specs/vcb-engine-v2-log.md).
# Run from engine/, with cargo on the PATH and SDKROOT set as the README says:
#     sh tools/build-pgo.sh
# Three steps: a build that records what it runs, a training run with it (day 1 part 1 on the engine's own
# fixture board, so nothing outside engine/ is needed), and the real build that uses the record. Needs `rustup component add llvm-tools`. The record is not kept in git.
# Without this script, `cargo build --release` still gives a correct engine, a few percent slower.
set -e
here=$(pwd)
data="$here/target/pgo-data"
profdata=$(find "$(rustc --print sysroot)" -name llvm-profdata | head -1)
[ -n "$profdata" ] || { echo "llvm-profdata not found: rustup component add llvm-tools" >&2; exit 1; }
rm -rf "$data" && mkdir -p "$data"

echo "1/3 the recording build"
RUSTFLAGS="-Cprofile-generate=$data -C target-cpu=native" cargo build --release --target-dir target/pgo-gen 2>&1 | tail -1

echo "2/3 the training run"
# One program on the engine's own fixture board is enough: trained on this alone, the build is 1.12 times
# faster over all ten benchmark programs, the two other CPU designs among them (1.14 when trained on all ten).
target/pgo-gen/release/vcb run tests/boards/cpu-aoc1p1.vcb --ticks 60000000 > /dev/null 2>&1 || { echo "the training run failed" >&2; exit 1; }
"$profdata" merge -o "$data/merged.profdata" "$data"

echo "3/3 the build that uses the record"
RUSTFLAGS="-Cprofile-use=$data/merged.profdata -C target-cpu=native" cargo build --release 2>&1 | tail -1
echo "done: target/release/vcb"
