#!/usr/bin/env python3
"""Deterministic fixture generator for the lua differential test.

Every fixture is a plain-text Lua chunk that is loaded via
`luaL_loadstring` and executed via `lua_pcall`. Fixtures are committed
alongside this script so offline test runs do not need Python. Re-run
this script whenever the corpus changes to keep it reproducible.
"""
import os

FIXTURE_DIR = os.path.dirname(os.path.abspath(__file__))
CORPUS_DIR = os.path.join(FIXTURE_DIR, "lua_corpus")
os.makedirs(CORPUS_DIR, exist_ok=True)

# name -> Lua source. Every chunk returns a value so oracle tests can
# inspect the top-of-stack result after wr_open succeeds.
FIXTURES = {
    # Sanity fixture — the Phase-0 green-light oracle test opens this one.
    "hello.dat": 'return "hello, world"\n',

    # Integer arithmetic — exercises the VM beyond constant loading.
    "arithmetic.dat": "return 2 + 3 * 4\n",

    # String concatenation — touches the string library.
    "concat.dat": 'return "foo" .. "-" .. "bar"\n',

    # Table length — touches the table library.
    "table_len.dat": "local t = {10, 20, 30, 40, 50}\nreturn #t\n",

    # For-loop accumulator — exercises control flow + locals.
    "for_sum.dat": (
        "local s = 0\n"
        "for i = 1, 10 do s = s + i end\n"
        "return s\n"
    ),

    # stdlib call — exercises luaL_openlibs having run.
    "stdlib.dat": 'return string.upper("lua")\n',
}


def write_fixture(name: str, body: str) -> str:
    path = os.path.join(CORPUS_DIR, name)
    # Binary mode with explicit LF newlines keeps the file byte-identical
    # across platforms and avoids any text-mode CRLF rewriting on Windows.
    with open(path, "wb") as f:
        f.write(body.encode("utf-8"))
    return path


def main() -> None:
    print(f"Writing fixtures to {CORPUS_DIR}")
    for name, body in sorted(FIXTURES.items()):
        path = write_fixture(name, body)
        print(f"  {os.path.relpath(path, FIXTURE_DIR)} ({len(body)} bytes)")


if __name__ == "__main__":
    main()
