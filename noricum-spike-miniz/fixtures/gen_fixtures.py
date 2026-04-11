#!/usr/bin/env python3
"""Generate deterministic .zip fixtures for the differential test harness.

Every fixture is deterministic: fixed timestamps, fixed RNG seeds, no per-run
variation. Commit both this script AND the generated .zip files — determinism
is the insurance policy, committed files are the working artifact.
"""

import os
import random
import zipfile

FIXTURE_DIR = os.path.dirname(os.path.abspath(__file__))
ZIP_DIR = os.path.join(FIXTURE_DIR, "zip_corpus")
os.makedirs(ZIP_DIR, exist_ok=True)

# All fixtures use this timestamp for determinism.
EPOCH = (2026, 1, 1, 0, 0, 0)


def _info(name, compress_type=zipfile.ZIP_DEFLATED):
    info = zipfile.ZipInfo(name, EPOCH)
    info.compress_type = compress_type
    return info


def _write(name, fn, *, allow_zip64=True):
    path = os.path.join(ZIP_DIR, name)
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED, allowZip64=allow_zip64) as z:
        fn(z)
    size = os.path.getsize(path)
    print(f"  {name:24s} {size:>10d} bytes")


# ---------- fixture definitions ----------


def f_hello(z):
    z.writestr(_info("hello.txt"), b"hello\n")


def f_empty(z):
    pass  # zero entries


def f_multi_small(z):
    for i in range(5):
        z.writestr(_info(f"file{i}.txt"), f"content of file {i}\n".encode())


def f_large_single(z):
    r = random.Random(42)
    data = bytes(r.randint(0, 255) for _ in range(2 * 1024 * 1024))  # 2 MiB
    z.writestr(_info("large.bin"), data)


def f_unicode_names(z):
    pairs = [
        ("nihongo.txt", "basic ascii\n"),
        ("日本語.txt", "japanese\n"),
        ("emoji_rocket.txt", "rocket\n"),
        ("accente.txt", "latin1-ish\n"),
    ]
    for name, content in pairs:
        z.writestr(_info(name), content.encode("utf-8"))


def f_nested_paths(z):
    for path in ["dir/a.txt", "dir/subdir/b.txt", "dir/subdir/deeper/c.txt", "other/d.txt"]:
        z.writestr(_info(path), f"content of {path}\n".encode())


def f_with_comment(z):
    z.comment = b"archive-level comment"
    for i in range(3):
        info = _info(f"f{i}.txt")
        info.comment = f"entry comment {i}".encode()
        z.writestr(info, f"body {i}\n".encode())


def f_stored_only(z):
    z.writestr(_info("stored.txt", zipfile.ZIP_STORED), b"no compression here\n")
    z.writestr(_info("stored2.txt", zipfile.ZIP_STORED), b"also uncompressed\n")


def f_deflated(z):
    payload = b"this should compress well " * 256
    z.writestr(_info("deflated.txt"), payload)


def f_duplicate_names(z):
    for i in range(2):
        z.writestr(_info("dup.txt"), f"version {i}\n".encode())
    z.writestr(_info("unique.txt"), b"unique entry\n")


def f_misordered(z):
    # zipfile writes in insertion order; insertion order != alphabetical
    for name in ["zebra.txt", "alpha.txt", "middle.txt", "banana.txt"]:
        z.writestr(_info(name), f"{name} body\n".encode())


def f_long_filename(z):
    name = "a" * 200 + ".txt"
    z.writestr(_info(name), b"long-name body\n")


def f_aligned(z):
    for i in range(3):
        z.writestr(_info(f"aligned{i}.txt", zipfile.ZIP_STORED), f"aligned {i}\n".encode())


def f_zip64_forced(path):
    # Force the zip64 extension fields on even for a tiny file by using force_zip64.
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED, allowZip64=True) as z:
        info = _info("in_zip64.txt")
        with z.open(info, "w", force_zip64=True) as fp:
            fp.write(b"small file with zip64 extensions forced on\n")


FIXTURES = [
    ("hello.zip",            f_hello),
    ("empty.zip",             f_empty),
    ("multi_small.zip",       f_multi_small),
    ("large_single.zip",      f_large_single),
    ("unicode_names.zip",     f_unicode_names),
    ("nested_paths.zip",      f_nested_paths),
    ("with_comment.zip",      f_with_comment),
    ("stored_only.zip",       f_stored_only),
    ("deflated.zip",          f_deflated),
    ("duplicate_names.zip",   f_duplicate_names),
    ("misordered.zip",        f_misordered),
    ("long_filename.zip",     f_long_filename),
    ("aligned.zip",           f_aligned),
]


def main():
    print(f"Writing fixtures to {ZIP_DIR}")
    for name, fn in FIXTURES:
        _write(name, fn)
    f_zip64_forced(os.path.join(ZIP_DIR, "zip64.zip"))
    print(f"  {'zip64.zip':24s} {os.path.getsize(os.path.join(ZIP_DIR, 'zip64.zip')):>10d} bytes")
    print(f"\n{len(FIXTURES) + 1} fixtures written.")


if __name__ == "__main__":
    main()
