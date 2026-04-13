#!/bin/sh
# Run each bench script N times on both runtimes and report the
# best wall-clock time for each. Also asserts stdout is identical.
set -u
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
RUST_DBG="$ROOT/target/debug/lua"
RUST_REL="$ROOT/target/release/lua"
C="${LUA_C:-/tmp/lua-ref/lua}"
DIR="$(dirname "$0")"
N="${N:-3}"

if [ ! -x "$RUST_REL" ]; then
  (cd "$ROOT" && cargo build --release --bin lua >/dev/null 2>&1)
fi

best() {
  bin="$1" ; script="$2"
  best_us=999999999
  for i in $(seq 1 $N); do
    t0=$(date +%s%N)
    out=$("$bin" "$script" 2>/dev/null)
    t1=$(date +%s%N)
    us=$(( (t1 - t0) / 1000 ))
    if [ "$us" -lt "$best_us" ]; then best_us="$us"; fi
  done
  printf "%s\t%s" "$best_us" "$out"
}

printf "%-22s  %12s  %12s  %12s  %s\n" "bench" "C(us)" "Rust-rel(us)" "ratio" "match?"
echo "------------------------------------------------------------------------------"
for f in "$DIR"/bench_*.lua; do
  base=$(basename "$f" .lua)
  c_line=$(best "$C" "$f")
  r_line=$(best "$RUST_REL" "$f")
  c_us=$(echo "$c_line" | cut -f1)
  c_out=$(echo "$c_line" | cut -f2-)
  r_us=$(echo "$r_line" | cut -f1)
  r_out=$(echo "$r_line" | cut -f2-)
  if [ "$c_out" = "$r_out" ]; then match="OK"; else match="DIFF"; fi
  ratio="-"
  if [ "$c_us" -gt 0 ]; then
    ratio=$(awk "BEGIN { printf \"%.2fx\", $r_us / $c_us }")
  fi
  printf "%-22s  %12s  %12s  %12s  %s\n" "$base" "$c_us" "$r_us" "$ratio" "$match"
done
