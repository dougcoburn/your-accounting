#!/usr/bin/env bash
# Prove each privacy guard fails on a planted synthetic violation and passes
# on the clean tree. Pass one guard name to test just that guard.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec python3 "$SCRIPT_DIR/privacy_guard.py" test "$@"
