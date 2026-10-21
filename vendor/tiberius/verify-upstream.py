#!/usr/bin/env python3
import difflib
import hashlib
from pathlib import Path
import sys
import tarfile

CHECKSUM = "e07324791de2bdaed058af4aa433a0b5ec5c0beac407a9797419c19805df2415"
root = Path(__file__).resolve().parent
if len(sys.argv) < 2:
    raise SystemExit("usage: verify-upstream.py /path/to/tiberius-0.13.0.crate [--write]")
archive = Path(sys.argv[1])
if hashlib.sha256(archive.read_bytes()).hexdigest() != CHECKSUM:
    raise SystemExit("The archive does not match the pinned tiberius 0.13.0 checksum")
parts = []
unchanged = 0
changed = []
included = {"Cargo.toml", "Cargo.toml.orig", "README.md", "LICENSE-MIT.txt", "LICENSE-APACHE.txt", ".cargo_vcs_info.json"}
source_files = set()
with tarfile.open(archive) as source:
    for item in sorted(source.getmembers(), key=lambda item: item.name):
        relative = item.name.split("/", 1)[-1]
        local = root / relative
        if not item.isfile() or not (relative.startswith("src/") or relative in included):
            continue
        source_files.add(relative)
        if not local.is_file():
            raise SystemExit(f"Missing upstream file: {relative}")
        before = source.extractfile(item).read()
        after = local.read_bytes()
        if before == after:
            unchanged += 1
            continue
        changed.append(relative)
        parts.extend(difflib.unified_diff(before.decode().splitlines(True), after.decode().splitlines(True),
                                          fromfile="a/" + relative, tofile="b/" + relative))
for local in (root / "src").rglob("*"):
    if local.is_file() and local.relative_to(root).as_posix() not in source_files:
        raise SystemExit(f"Unrecorded source file: {local}")
actual = "".join(parts)
if "--write" in sys.argv[2:]:
    (root / "UPSTREAM.patch").write_text(actual)
elif actual != (root / "UPSTREAM.patch").read_text():
    raise SystemExit("The source delta differs from UPSTREAM.patch")
print(f"Verified archive SHA-256; {unchanged} unchanged files; changed files: {', '.join(changed)}")
print(f"Patch lines: {len(actual.splitlines())}")
