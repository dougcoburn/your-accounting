#!/usr/bin/env bash
# Fail if any tracked text file contains a denylisted term.
#
# Committed plaintext terms are synthetic placeholders in
# scripts/denylist.placeholders. Real terms are read from the gitignored
# file private/denylist.txt and from the YA_DENYLIST environment variable.
# Historical terms remain SHA-256 digests in scripts/denylist.sha256.
# Matching terms are not printed.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec python3 "$SCRIPT_DIR/privacy_guard.py" denylist
