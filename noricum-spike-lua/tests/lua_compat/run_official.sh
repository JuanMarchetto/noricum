#!/bin/sh
# Run the official Lua 5.4 test suite (downloaded to /tmp) on
# both runtimes. Per-file: pass when the runtime exits with a
# zero status and emits no error to stderr. Many official tests
# exercise highly internal features and are expected to fail
# without further hardening; this just gives us a coverage map.
set -u
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
RUST="$ROOT/target/release/lua"
C="${LUA_C:-/tmp/lua-ref/lua}"
TESTS="${TESTS:-/tmp/lua-5.4.4-tests}"

if [ ! -d "$TESTS" ]; then
  echo "official test suite not found at $TESTS"
  exit 2
fi

c_pass=0; c_fail=0; r_pass=0; r_fail=0
total=0

for f in "$TESTS"/*.lua; do
  base=$(basename "$f" .lua)
  # Skip the entry-point harness (all.lua chains every other test
  # which would compound failures and obscure per-file results).
  case "$base" in
    all|main) continue ;;
  esac
  total=$((total + 1))
  c_status=$(cd "$TESTS" && timeout 15 "$C" "$base.lua" >/dev/null 2>&1 ; echo $?)
  r_status=$(cd "$TESTS" && timeout 15 "$RUST" "$base.lua" >/dev/null 2>&1 ; echo $?)
  if [ "$c_status" = "0" ]; then c_pass=$((c_pass+1)); else c_fail=$((c_fail+1)); fi
  if [ "$r_status" = "0" ]; then r_pass=$((r_pass+1)); else r_fail=$((r_fail+1)); fi
  c_mark="OK"
  [ "$c_status" = "0" ] || c_mark="FAIL($c_status)"
  r_mark="OK"
  [ "$r_status" = "0" ] || r_mark="FAIL($r_status)"
  printf "  %-22s  C:%-12s  Rust:%s\n" "$base" "$c_mark" "$r_mark"
done
echo
echo "Total: $total"
echo "C reference: $c_pass passed, $c_fail failed"
echo "Rust port  : $r_pass passed, $r_fail failed"
