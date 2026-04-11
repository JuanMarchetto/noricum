#!/usr/bin/env python3
"""Generate deterministic fixtures for the lua differential test."""
import os
# TODO: import the Python library that produces your target's format
# (zipfile, tarfile, struct, etc.)

FIXTURE_DIR = os.path.dirname(os.path.abspath(__file__))
CORPUS_DIR = os.path.join(FIXTURE_DIR, "lua_corpus")
os.makedirs(CORPUS_DIR, exist_ok=True)

def write_sanity_fixture():
    """The simplest fixture that exercises the library's basic read path."""
    path = os.path.join(CORPUS_DIR, "hello.dat")
    # TODO: write the sanity fixture for this format
    with open(path, "wb") as f:
        f.write(b"hello\n")
    return path

def main():
    print(f"Writing fixtures to {CORPUS_DIR}")
    write_sanity_fixture()
    print("TODO: add more fixtures covering edge cases")

if __name__ == "__main__":
    main()
