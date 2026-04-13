#!/bin/sh
# Build the Rust cdylib then link a C host against it.
set -e
cd "$(dirname "$0")/.."
cargo build -p noricum-lua-ffi 2>&1 | tail -2
cc -O2 -Wall ffi/demo.c -L target/debug -lnoricum_lua -Wl,-rpath,"$(pwd)/target/debug" -o ffi/demo
./ffi/demo
