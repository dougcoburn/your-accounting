#!/usr/bin/env python3
"""Data guard: keep real financial data out of git.

Checks tracked or staged files for:

  * CSV or JSON that does not carry the synthetic marker
  * CSV files that do not live under a ``fixtures/`` directory
  * account-number-like digit runs longer than 4 digits

Findings report file and line only; matched text is never printed.

Marker convention
-----------------
CSV
    A comment line whose first token after ``#`` is ``synthetic``.
    Put it on the first line: ``# synthetic``.

JSON
    Either a top-level ``"_synthetic": true`` key (boolean, valid JSON),
    or a leading comment line containing ``# synthetic`` (JSONC), for
    example ``// # synthetic``. JSON arrays cannot carry the key, so they
    must use the comment form.

Modes
-----
  (default)   inspect staged paths (``git diff --cached``); for pre-commit
  --all       inspect every tracked file; for CI
"""

from __future__ import annotations

import json
import re
import subprocess
import sys
from collections.abc import Iterable, Iterator
from datetime import date
from pathlib import Path

FIXTURES_DIR = "fixtures"

# Account-number scans follow the same file scope as the U14 privacy guard:
# text data files, markdown, anything under docs/, and prototypes/.
ACCOUNT_TEXT_DATA_EXTENSIONS = {
    ".csv",
    ".tsv",
    ".json",
    ".ofx",
    ".qfx",
    ".qif",
    ".qbo",
    ".ofc",
    ".iif",
}


def account_scan_target(path: str) -> bool:
    posix = _posix(path)
    suffix = Path(posix).suffix.lower()
    if suffix in ACCOUNT_TEXT_DATA_EXTENSIONS or suffix in {".md", ".rst"}:
        return True
    parts = Path(posix).parts
    if "docs" in parts[:-1]:
        return True
    return bool(parts) and parts[0] == "prototypes"


# Consecutive 5+ digit runs. Underscore is treated as part of a token so
# identifiers such as je_00000016 and Rust literals like 10000_00 are
# ignored, as are lockfile hashes and other alphanumeric tokens.
DIGIT_RUN = re.compile(r"(?<![0-9A-Za-z_])\d{5,}(?![0-9A-Za-z_])")

# Grouped account numbers: three or more space/dash-separated groups, or
# a group of 5+ digits next to another numeric group. Two short groups
# such as 0001-0008 are treated as a document range, not an account.
GROUPED_RUN = re.compile(
    r"(?<![0-9A-Za-z_])"
    r"(?:"
    r"\d{1,4}(?:[ -]\d{1,4}){2,}"
    r"|"
    r"\d{5,}(?:[ -]\d+)+"
    r"|"
    r"\d+[ -]\d{5,}"
    r")"
    r"(?![0-9A-Za-z_])"
)

# ISO dates and datetimes, plus slash-separated dates.
DATE_RE = re.compile(
    r"""
    (?<![0-9A-Za-z])
    (?:
        \d{4}-\d{2}-\d{2}
        (?:[T ]\d{2}:\d{2}(?::\d{2})?(?:\.\d+)?(?:Z|[+-]\d{2}:?\d{2})?)?
      | \d{1,2}/\d{1,2}/\d{2,4}
    )
    (?![0-9A-Za-z])
    """,
    re.VERBOSE,
)

# Currency amounts, grouped thousands, decimal money or rates (any
# fractional length, e.g. 1.1850000), ISO-code-adjacent minor units,
# debit/credit labels, "N minor units", and Rust underscore literals
# such as 10000_00.
ISO4217 = (
    r"(?:USD|EUR|JPY|GBP|CHF|CAD|AUD|NZD|SEK|NOK|DKK|"
    r"CNY|HKD|SGD|INR|KRW|MXN|BRL|ZAR)"
)
AMOUNT_RE = re.compile(
    rf"""
    (?:
        [\$€£¥]\s*-?\d{{1,3}}(?:,\d{{3}})+(?:\.\d+)?
      | [\$€£¥]\s*-?\d+(?:\.\d+)?
      | (?<![0-9A-Za-z])-?\d{{1,3}}(?:,\d{{3}})+(?:\.\d+)?(?![0-9A-Za-z])
      | (?<![0-9A-Za-z])-?\d+\.\d+(?![0-9A-Za-z])
      | (?<![0-9A-Za-z])\d+(_\d+)+(?![0-9A-Za-z])
      | (?<![0-9A-Za-z]){ISO4217}\s*[|'\":,]*\s*-?\d{{5,}}
      | (?<![0-9A-Za-z])-?\d{{5,}}\s*{ISO4217}(?![0-9A-Za-z])
      | (?<![0-9A-Za-z])(?:debits?|credits?)\s+-?\d{{5,}}
      | (?<![0-9A-Za-z])-?\d{{5,}}\s*minor\b
    )
    """,
    re.VERBOSE | re.IGNORECASE,
)

# 5+ digit terms in arithmetic or equality, such as 118500 / 100000
# or 108000 + 10500 = 118500. Account numbers are not written this way.
ARITH_AMOUNT_RE = re.compile(
    r"(?<![0-9A-Za-z])-?\d{5,}(?:\s*[+/*x=]\s*-?\d+(?:,\d{3})*(?:\.\d+)?)+"
)
# Remainder after a currency amount was already masked: "= " plus 5+ digits.
EQUALS_AMOUNT_RE = re.compile(r"=\s*-?\d{5,}")
# Markdown table cell that is only a 5+ digit integer (minor-unit columns).
TABLE_MINOR_RE = re.compile(r"\|\s*\d{5,}\s*\|")

VERSION_RE = re.compile(
    r"(?<![0-9A-Za-z])\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?(?![0-9A-Za-z])"
)

CSV_MARKER_LINE = re.compile(r"^\s*#\s*synthetic\b", re.IGNORECASE)
JSONC_MARKER_LINE = re.compile(
    r"^\s*(?://|#)\s*(?:#\s*)?synthetic\b", re.IGNORECASE
)
SYNTHETIC_KEY_RE = re.compile(r"""["']_synthetic["']\s*:\s*true\b""")


def _posix(path: str) -> str:
    return path.replace("\\", "/")


def is_csv(path: str) -> bool:
    return _posix(path).lower().endswith(".csv")


def is_json(path: str) -> bool:
    return _posix(path).lower().endswith(".json")


def under_fixtures(path: str) -> bool:
    parts = _posix(path).split("/")
    return FIXTURES_DIR in parts[:-1]


def path_problems(path: str) -> list[str]:
    problems: list[str] = []
    if is_csv(path) and not under_fixtures(path):
        problems.append("CSV files must live under a fixtures/ directory")
    return problems


def has_synthetic_marker(path: str, text: str) -> bool:
    if text.startswith("\ufeff"):
        text = text[1:]
    if is_csv(path):
        return any(CSV_MARKER_LINE.match(line) for line in text.splitlines())
    if is_json(path):
        for line in text.splitlines():
            if not line.strip():
                continue
            if JSONC_MARKER_LINE.match(line) or CSV_MARKER_LINE.match(line):
                return True
            break
        if SYNTHETIC_KEY_RE.search(text):
            return True
        payload = _json_without_leading_comments(text)
        if payload is not None and isinstance(payload, dict):
            return payload.get("_synthetic") is True
        return False
    return True


def _json_without_leading_comments(text: str) -> object | None:
    lines = text.splitlines()
    kept: list[str] = []
    started = False
    for line in lines:
        if not started and (
            not line.strip()
            or line.lstrip().startswith("//")
            or line.lstrip().startswith("#")
        ):
            continue
        started = True
        kept.append(line)
    try:
        return json.loads("\n".join(kept))
    except json.JSONDecodeError:
        return None


def _mask(pattern: re.Pattern[str], line: str) -> str:
    return pattern.sub(lambda m: " " * len(m.group(0)), line)


def _is_yyyymmdd(digits: str) -> bool:
    if len(digits) != 8 or not digits.isdigit():
        return False
    try:
        date(int(digits[:4]), int(digits[4:6]), int(digits[6:8]))
    except ValueError:
        return False
    return True


def line_problems(line: str) -> list[str]:
    masked = _mask(DATE_RE, line)
    masked = _mask(VERSION_RE, masked)
    masked = _mask(AMOUNT_RE, masked)
    masked = _mask(ARITH_AMOUNT_RE, masked)
    masked = _mask(EQUALS_AMOUNT_RE, masked)
    masked = _mask(TABLE_MINOR_RE, masked)
    # SQL / script literals on amount columns: comma, then 5+ digits, then )
    if re.search(r"amount_minor", line, re.IGNORECASE):
        masked = re.sub(
            r",\s*\d{5,}(?=\s*\))",
            lambda m: " " * len(m.group(0)),
            masked,
        )
    for pattern in (DIGIT_RUN, GROUPED_RUN):
        for match in pattern.finditer(masked):
            digits = re.sub(r"[ -]", "", match.group(0))
            if len(digits) > 4 and not _is_yyyymmdd(digits):
                return ["account-number-like digit run longer than 4 digits"]
    return []


def scan_lines(path: str, numbered: Iterable[tuple[int, str]]) -> Iterator[str]:
    for lineno, text in numbered:
        for problem in line_problems(text):
            yield f"{path}:{lineno}: {problem}"


def inspect_file(path: str, data: bytes) -> list[str]:
    findings = [f"{path}: {problem}" for problem in path_problems(path)]
    if b"\0" in data:
        return findings
    text = data.decode("utf-8", "replace")
    if is_csv(path) or is_json(path):
        if not has_synthetic_marker(path, text):
            findings.append(f"{path}: missing synthetic marker")
    if account_scan_target(path):
        lines = text.splitlines()
        findings.extend(scan_lines(path, enumerate(lines, start=1)))
    return findings


def _git(*args: str, cwd: str | Path | None = None) -> bytes:
    return subprocess.run(
        ["git", *args],
        check=True,
        capture_output=True,
        cwd=cwd,
    ).stdout


def git_root(cwd: str | Path | None = None) -> Path:
    out = _git("rev-parse", "--show-toplevel", cwd=cwd).decode().strip()
    return Path(out)


def staged_findings(repo: Path | None = None) -> list[str]:
    root = repo or git_root()
    names = _git(
        "diff", "--cached", "--name-only", "--diff-filter=ACMR", "-z", cwd=root
    ).split(b"\0")
    paths = [n.decode("utf-8", "replace") for n in names if n]
    findings: list[str] = []
    for path in paths:
        blob = subprocess.run(
            ["git", "show", f":{path}"],
            cwd=root,
            capture_output=True,
        )
        if blob.returncode != 0:
            continue
        findings.extend(inspect_file(path, blob.stdout))
    return findings


def tracked_findings(repo: Path | None = None) -> list[str]:
    root = repo or git_root()
    names = _git("ls-files", "-z", cwd=root).split(b"\0")
    paths = [n.decode("utf-8", "replace") for n in names if n]
    findings: list[str] = []
    for path in paths:
        try:
            data = (root / path).read_bytes()
        except OSError:
            continue
        findings.extend(inspect_file(path, data))
    return findings


def main(argv: list[str]) -> int:
    if "-h" in argv or "--help" in argv:
        print(__doc__)
        return 0
    findings = tracked_findings() if "--all" in argv else staged_findings()
    if findings:
        print(
            "data guard: BLOCKED - possible real financial data:",
            file=sys.stderr,
        )
        for finding in findings:
            print(f"  {finding}", file=sys.stderr)
        print(
            "Remove or unstage these. Matched text is intentionally not shown.",
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
