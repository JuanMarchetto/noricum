#!/bin/sh
# Run every .lua file under this directory on both the C reference
# Lua 5.5 and our Rust target/debug/lua. Diff stdout byte-for-byte.
# Exits 0 only when every file matches; prints summary either way.
set -u
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
RUST="$ROOT/target/debug/lua"
C="${LUA_C:-/tmp/lua-ref/lua}"
DIR="$(dirname "$0")"

if [ ! -x "$C" ]; then echo "C lua not found at $C"; exit 2; fi
if [ ! -x "$RUST" ]; then
  (cd "$ROOT" && cargo build --bin lua >/dev/null 2>&1)
fi

pass=0
fail=0
files=$(ls "$DIR"/*.lua | sort)
for f in $files; do
  base=$(basename "$f" .lua)
  out_c=$(timeout 10 "$C"    "$f" 2>&1)
  out_r=$(timeout 10 "$RUST" "$f" 2>&1)
  if [ "$out_c" = "$out_r" ]; then
    pass=$((pass+1))
    printf "  PASS  %s\n" "$base"
  else
    fail=$((fail+1))
    printf "  FAIL  %s\n" "$base"
    echo "---- C   ----"; echo "$out_c" | head -10
    echo "---- Rust ----"; echo "$out_r" | head -10
    echo "----"
  fi
done
echo
echo "Result: $pass passed, $fail failed"
exit $fail
