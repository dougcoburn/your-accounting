#!/usr/bin/env bash
# Fail if any tracked text file contains a denylisted word token.
# Terms are stored only as SHA-256 hashes of their lowercase form in
# scripts/denylist.sha256, so the forbidden words themselves are not in the repo.
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

HASH_FILE="$ROOT/scripts/denylist.sha256"
if [[ ! -f "$HASH_FILE" ]]; then
  echo "denylist: missing $HASH_FILE" >&2
  exit 2
fi

if ! command -v sha256sum >/dev/null 2>&1; then
  echo "denylist: sha256sum is required" >&2
  exit 2
fi
if ! command -v python3 >/dev/null 2>&1; then
  echo "denylist: python3 is required" >&2
  exit 2
fi

export HASH_FILE
python3 - "$HASH_FILE" << 'PY'
import hashlib
import os
import re
import subprocess
import sys
from pathlib import Path

hash_file = Path(sys.argv[1])
deny = set()
for raw in hash_file.read_text(encoding="utf-8").splitlines():
    line = raw.split("#", 1)[0].strip().lower()
    if not line:
        continue
    if not re.fullmatch(r"[0-9a-f]{64}", line):
        sys.stderr.write(f"denylist: invalid hash line in {hash_file.name}\n")
        sys.exit(2)
    deny.add(line)

if not deny:
    sys.stderr.write(f"denylist: no hashes in {hash_file}\n")
    sys.exit(2)

files = subprocess.check_output(
    ["git", "ls-files", "-z"], cwd=str(hash_file.parent.parent)
).split(b"\0")
files = [f.decode("utf-8") for f in files if f]

token_re = re.compile(r"[A-Za-z0-9]+")
hits = []

for rel in files:
    path = Path(rel)
    if not path.is_file():
        continue
    try:
        data = path.read_bytes()
    except OSError as err:
        sys.stderr.write(f"denylist: could not read {rel}: {err}\n")
        sys.exit(2)
    if b"\0" in data:
        continue
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError:
        try:
            text = data.decode("latin-1")
        except UnicodeDecodeError:
            continue
    for lineno, line in enumerate(text.splitlines(), start=1):
        for token in token_re.findall(line):
            digest = hashlib.sha256(token.lower().encode("ascii", "ignore")).hexdigest()
            # Tokens are almost always ASCII; if a token has non-ascii, hash the
            # lowercase UTF-8 instead so the comparison stays well-defined.
            if not token.isascii():
                digest = hashlib.sha256(token.lower().encode("utf-8")).hexdigest()
            if digest in deny:
                hits.append((rel, lineno, digest))

if hits:
    sys.stderr.write(
        f"denylist: {len(hits)} forbidden token(s) in tracked text files\n"
        "Matching terms are not printed. Offending locations:\n"
    )
    for rel, lineno, digest in hits:
        sys.stderr.write(f"  {rel}:{lineno}  hash {digest}\n")
    sys.exit(1)

print("denylist: ok")
PY
