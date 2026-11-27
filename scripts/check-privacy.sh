#!/usr/bin/env bash
# Run every U14 privacy guard against this repository.
# CI should call this script. Pass one guard name to run just that guard:
#   data-files, private, denylist, account-numbers, logs
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec python3 "$SCRIPT_DIR/privacy_guard.py" check "$@"
