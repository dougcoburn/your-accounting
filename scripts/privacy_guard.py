#!/usr/bin/env python3
"""U14 privacy guards.

One command for CI: scripts/check-privacy.sh (this file's ``check`` subcommand).
Planted-violation tests: scripts/test-privacy.sh (``test`` subcommand).

Guards:
  data-files        csv, qfx, ofx, pdf, db, and sqlite outside a fixtures/ dir;
                    json only when it is under fixtures/ or carries a synthetic marker
  private           private/ is gitignored and nothing under it is tracked
  denylist          placeholder, local, env, and hashed identifying terms
  account-numbers   account-number-like digit runs in tracked text data, docs, and prototypes/
  logs              redacting logger drops payee, description, and memo;
                    skip when the crate or cargo is absent
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from collections.abc import Mapping
from pathlib import Path

SCRIPT = Path(__file__).resolve()
GUARDS = ("data-files", "private", "denylist", "account-numbers", "logs")

# Placement guard. JSON is handled separately: it counts only under a fixtures
# directory, or when the file itself carries a synthetic-data marker.
REAL_DATA_EXTENSIONS = {
    ".csv",
    ".qfx",
    ".ofx",
    ".pdf",
    ".db",
    ".sqlite",
    ".sqlite3",
}
TEXT_DATA_EXTENSIONS = {
    ".csv",
    ".ofx",
    ".qfx",
}
BINARY_DATA_EXTENSIONS = REAL_DATA_EXTENSIONS - TEXT_DATA_EXTENSIONS
# Account-number scan covers these text data files wherever they are tracked,
# plus markdown, anything under a docs/ directory, and everything under prototypes/.
ACCOUNT_TEXT_DATA_EXTENSIONS = TEXT_DATA_EXTENSIONS | {
    ".tsv",
    ".json",
    ".qif",
    ".qbo",
    ".ofc",
    ".iif",
}
ACCOUNT_SKIP_SUFFIXES = BINARY_DATA_EXTENSIONS | {
    ".xls",
    ".xlsx",
    ".png",
    ".jpg",
    ".jpeg",
    ".gif",
    ".webp",
    ".ico",
    ".woff",
    ".woff2",
    ".zip",
    ".gz",
}
MARKERS = (b"# synthetic", b"<!-- synthetic -->", b"% synthetic")
PLACEHOLDER_FILE = "scripts/denylist.placeholders"
HASH_FILE = "scripts/denylist.sha256"
LOCAL_DENYLIST = "private/denylist.txt"
CRATE_MANIFEST = Path("accounting-core/crates/accounting-privacy/Cargo.toml")
WORKSPACE_MANIFEST = Path("accounting-core/Cargo.toml")

TOKEN_RE = re.compile(r"[A-Za-z0-9]+")
DIGIT_RUN = re.compile(r"(?<![\d.])(\d{5,})(?![\d.])")
GROUPED_ACCOUNT = re.compile(r"(?<!\d)(?:\d{4}[\s-]){2,}\d{4}(?!\d)")
IBAN = re.compile(r"^[A-Za-z]{2}\d{2}[A-Za-z0-9]{10,30}$")
MONEY = re.compile(
    r"\b(?:USD|EUR|JPY|GBP|minor|amount_minor|expected_amount_minor|debit|credit)\b",
    re.IGNORECASE,
)
# Unquoted or quoted integer that is the entire value of a *Minor JSON key.
JSON_MINOR_VALUE = re.compile(
    r'"(?P<key>[A-Za-z_][A-Za-z0-9_]*)"\s*:\s*(?:"(?P<qnum>\d+)"|(?P<bnum>\d+))(?!\d)'
)

# Extensions the planted test requires the guard to keep blocking outside fixtures/.
REQUIRED_DATA_EXTENSIONS = {
    ".csv",
    ".json",
    ".pdf",
    ".qfx",
    ".ofx",
    ".db",
    ".sqlite",
    ".sqlite3",
}


class Report:
    def __init__(
        self,
        name: str,
        errors: list[str] | None = None,
        config_error: str | None = None,
        skipped: str | None = None,
    ):
        self.name = name
        self.errors = errors or []
        self.config_error = config_error
        self.skipped = skipped

    def code(self) -> int:
        if self.config_error:
            return 2
        if self.errors:
            return 1
        return 0


class GitError(RuntimeError):
    pass


def git_files(root: Path) -> list[str]:
    """Tracked files plus untracked files that are not gitignored."""
    proc = subprocess.run(
        ["git", "ls-files", "-z", "-c", "-o", "--exclude-standard"],
        cwd=root,
        capture_output=True,
        check=False,
    )
    if proc.returncode != 0:
        detail = proc.stderr.decode("utf-8", "replace").strip() or "git ls-files failed"
        raise GitError(detail)
    return [part.decode("utf-8") for part in proc.stdout.split(b"\0") if part]


def read_text(path: Path) -> str | None:
    """Return text, or None when the file is binary."""
    try:
        data = path.read_bytes()
    except OSError as err:
        raise GitError(f"could not read {path}: {err}") from err
    if b"\0" in data:
        return None
    try:
        return data.decode("utf-8")
    except UnicodeDecodeError:
        return data.decode("latin-1")


def is_fixture_rel(rel: str) -> bool:
    """True when a parent directory is named fixtures, at any depth."""
    return "fixtures" in Path(rel).parts[:-1]


def marker_in(data: bytes) -> bool:
    return any(marker in data for marker in MARKERS)


def json_synthetic_marker(data: bytes) -> bool:
    """True for an in-file JSON synthetic marker.

    Accepted forms: a byte marker (``# synthetic``, an HTML comment, or a PDF
    ``% synthetic`` comment), or a top-level ``"_synthetic": true`` key.
    A nested ``_synthetic`` key does not count.
    """
    if marker_in(data):
        return True
    try:
        parsed = json.loads(data.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError):
        return False
    return isinstance(parsed, dict) and parsed.get("_synthetic") is True


def sidecar_marker(root: Path, rel: str, present: set[str]) -> bool:
    sidecar = rel + ".synthetic"
    if sidecar not in present:
        return False
    return b"# synthetic" in (root / sidecar).read_bytes()


def emit(report: Report) -> int:
    if report.config_error:
        print(f"{report.name}: {report.config_error}", file=sys.stderr)
        return 2
    if report.skipped and not report.errors:
        print(f"{report.name}: SKIP {report.skipped}")
        return 0
    if report.errors:
        if report.name == "denylist":
            print(
                f"denylist: {len(report.errors)} forbidden term(s) in tracked text files",
                file=sys.stderr,
            )
            print("Matching terms are not printed. Offending locations:", file=sys.stderr)
        else:
            print(f"{report.name}: {len(report.errors)} problem(s)", file=sys.stderr)
        for err in report.errors:
            print(f"  {err}", file=sys.stderr)
        return 1
    print(f"{report.name}: ok")
    return 0


def check_data_files(root: Path) -> Report:
    try:
        files = git_files(root)
    except GitError as err:
        return Report("data-files", config_error=str(err))
    present = set(files)
    errors: list[str] = []
    for rel in files:
        suffix = Path(rel).suffix.lower()
        if suffix not in REAL_DATA_EXTENSIONS and suffix != ".json":
            continue
        path = root / rel
        try:
            data = path.read_bytes()
        except OSError as err:
            return Report("data-files", config_error=f"could not read {rel}: {err}")
        try:
            side = sidecar_marker(root, rel, present)
        except OSError as err:
            return Report("data-files", config_error=f"could not read {rel}.synthetic: {err}")
        if suffix == ".json":
            marked = json_synthetic_marker(data) or side
            if not is_fixture_rel(rel) and not marked:
                continue
        if not is_fixture_rel(rel):
            errors.append(f"{rel} is outside fixtures/")
            continue
        if suffix in TEXT_DATA_EXTENSIONS:
            if not marker_in(data):
                errors.append(f"{rel} is missing a synthetic marker")
            continue
        if suffix == ".json":
            if not json_synthetic_marker(data) and not side:
                errors.append(f"{rel} is missing a synthetic marker")
            continue
        if not marker_in(data) and not side:
            errors.append(f"{rel} is missing a synthetic marker")
    return Report("data-files", errors)


def check_private(root: Path) -> Report:
    probe = "private/statement.csv"
    proc = subprocess.run(
        ["git", "check-ignore", "-q", "--no-index", probe],
        cwd=root,
        capture_output=True,
        check=False,
    )
    if proc.returncode not in (0, 1):
        detail = proc.stderr.decode("utf-8", "replace").strip() or "git check-ignore failed"
        return Report("private", config_error=detail)
    errors: list[str] = []
    if proc.returncode != 0:
        errors.append(f"{probe} is not ignored by .gitignore")
    try:
        files = git_files(root)
    except GitError as err:
        return Report("private", config_error=str(err))
    for rel in files:
        if rel == "private" or rel.startswith("private/"):
            errors.append(f"tracked file under private/: {rel}")
    return Report("private", errors)


def term_lines(path: Path) -> list[str]:
    text = path.read_text(encoding="utf-8")
    terms: list[str] = []
    for raw in text.splitlines():
        line = raw.split("#", 1)[0].strip()
        if line:
            terms.append(line)
    return terms


def compile_term(term: str) -> re.Pattern[str]:
    body = re.escape(term.strip())
    return re.compile(rf"(?<![A-Za-z0-9]){body}(?![A-Za-z0-9])", re.IGNORECASE)


def hash_token(token: str) -> str:
    if token.isascii():
        encoded = token.lower().encode("ascii", "ignore")
    else:
        encoded = token.lower().encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def load_hashes(path: Path) -> tuple[set[str], str | None]:
    if not path.is_file():
        return set(), f"missing {HASH_FILE}"
    hashes: set[str] = set()
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.split("#", 1)[0].strip().lower()
        if not line:
            continue
        if not re.fullmatch(r"[0-9a-f]{64}", line):
            return set(), f"invalid hash line in {HASH_FILE}"
        hashes.add(line)
    if not hashes:
        return set(), f"no hashes in {HASH_FILE}"
    return hashes, None


def load_plaintext_terms(root: Path, env: Mapping[str, str]) -> tuple[list[str], str | None]:
    placeholder_path = root / PLACEHOLDER_FILE
    if not placeholder_path.is_file():
        return [], f"missing {PLACEHOLDER_FILE}"
    terms = term_lines(placeholder_path)
    if not terms:
        return [], f"no placeholder terms in {PLACEHOLDER_FILE}"
    local = root / LOCAL_DENYLIST
    if local.is_file():
        terms.extend(term_lines(local))
    file_var = env.get("YA_DENYLIST_FILE", "").strip()
    if file_var:
        extra = Path(file_var)
        if not extra.is_file():
            return [], "YA_DENYLIST_FILE does not exist"
        terms.extend(term_lines(extra))
    for raw in env.get("YA_DENYLIST", "").splitlines():
        line = raw.split("#", 1)[0].strip()
        if line:
            terms.append(line)
    return terms, None


def check_denylist(root: Path, env: Mapping[str, str] | None = None) -> Report:
    # ``env`` supplies YA_DENYLIST only. git still uses the process environment.
    if env is None:
        env = os.environ
    hashes, hash_error = load_hashes(root / HASH_FILE)
    if hash_error:
        return Report("denylist", config_error=hash_error)
    terms, term_error = load_plaintext_terms(root, env)
    if term_error:
        return Report("denylist", config_error=term_error)
    patterns = [compile_term(term) for term in terms]
    try:
        files = git_files(root)
    except GitError as err:
        return Report("denylist", config_error=str(err))
    errors: list[str] = []
    for rel in files:
        path = root / rel
        if not path.is_file():
            continue
        try:
            text = read_text(path)
        except GitError as err:
            return Report("denylist", config_error=str(err))
        if text is None:
            continue
        skip_plaintext = rel == PLACEHOLDER_FILE
        for lineno, line in enumerate(text.splitlines(), start=1):
            if not skip_plaintext and any(pattern.search(line) for pattern in patterns):
                errors.append(f"{rel}:{lineno}")
            for token in TOKEN_RE.findall(line):
                digest = hash_token(token)
                if digest in hashes:
                    errors.append(f"{rel}:{lineno}  hash {digest}")
    return Report("denylist", errors)


def minor_json_value_spans(line: str) -> set[tuple[int, int]]:
    """Spans of 1-6 digit values of JSON keys whose name ends in ``minor``.

    ``openingMinor``, ``fromAmountMinor``, and ``amount_minor`` are minor-unit
    money fields. A longer run is not exempt: it can hide an account number.
    """
    spans: set[tuple[int, int]] = set()
    for match in JSON_MINOR_VALUE.finditer(line):
        if not match.group("key").lower().endswith("minor"):
            continue
        group = "qnum" if match.group("qnum") is not None else "bnum"
        if len(match.group(group)) > 6:
            continue
        spans.add(match.span(group))
    return spans


def account_line_violation(line: str, rel: str) -> bool:
    if GROUPED_ACCOUNT.search(line):
        return True
    exempt = minor_json_value_spans(line)
    strict = is_fixture_rel(rel)
    for match in DIGIT_RUN.finditer(line):
        if (match.start(1), match.end(1)) in exempt:
            continue
        run = match.group(1)
        token = token_at(line, match.start(1), match.end(1))
        if IBAN.fullmatch(token):
            return True
        if any(char.isalpha() for char in token):
            continue
        if strict:
            return True
        if len(run) <= 6 and MONEY.search(line):
            continue
        return True
    return False


def token_at(line: str, start: int, end: int) -> str:
    left = start
    while left > 0 and (line[left - 1].isalnum() or line[left - 1] == "_"):
        left -= 1
    right = end
    while right < len(line) and (line[right].isalnum() or line[right] == "_"):
        right += 1
    return line[left:right]


def account_scan_target(rel: str) -> bool:
    """Tracked text data, docs, and prototype files."""
    suffix = Path(rel).suffix.lower()
    if suffix in ACCOUNT_SKIP_SUFFIXES:
        return False
    if suffix in ACCOUNT_TEXT_DATA_EXTENSIONS or suffix in {".md", ".rst"}:
        return True
    parts = Path(rel).parts
    if "docs" in parts[:-1]:
        return True
    return bool(parts) and parts[0] == "prototypes"


def check_account_numbers(root: Path) -> Report:
    try:
        files = git_files(root)
    except GitError as err:
        return Report("account-numbers", config_error=str(err))
    errors: list[str] = []
    for rel in files:
        if not account_scan_target(rel):
            continue
        path = root / rel
        if not path.is_file():
            continue
        try:
            text = read_text(path)
        except GitError as err:
            return Report("account-numbers", config_error=str(err))
        if text is None:
            continue
        for lineno, line in enumerate(text.splitlines(), start=1):
            if account_line_violation(line, rel):
                errors.append(f"{rel}:{lineno} account-number-like digit run")
    return Report("account-numbers", errors)


_CARGO_LOOKUP = object()


def check_logs(root: Path, cargo_bin: str | None | object = _CARGO_LOOKUP) -> Report:
    """Run the redaction tests, or skip when cargo or the crate is absent.

    The default looks up ``cargo`` on ``PATH``. Tests pass ``cargo_bin=None``
    to simulate a machine without cargo. A present crate whose redaction test
    fails is still a failure.
    """
    if cargo_bin is _CARGO_LOOKUP:
        resolved = shutil.which("cargo")
    elif isinstance(cargo_bin, str) and cargo_bin:
        resolved = cargo_bin
    else:
        resolved = None
    crate_manifest = root / CRATE_MANIFEST
    missing: list[str] = []
    if not resolved:
        missing.append("cargo is absent")
    if not crate_manifest.is_file():
        missing.append("accounting-privacy crate is absent")
    if missing:
        return Report("logs", skipped="; ".join(missing))
    manifest = root / WORKSPACE_MANIFEST
    if not manifest.is_file():
        return Report("logs", errors=["accounting-privacy tests failed", "missing accounting-core/Cargo.toml"])
    assert resolved is not None
    proc = subprocess.run(
        [
            resolved,
            "test",
            "--manifest-path",
            str(manifest),
            "-p",
            "accounting-privacy",
            "--",
            "--test-threads=8",
        ],
        cwd=root / "accounting-core",
        check=False,
        capture_output=True,
        text=True,
    )
    sys.stdout.write(proc.stdout)
    sys.stderr.write(proc.stderr)
    combined = proc.stdout + proc.stderr
    required = (
        "clean_log_redacts_synthetic_transaction ... ok",
        "planted_unredacted_log_fails_the_guard ... ok",
    )
    missing = [name for name in required if name not in combined]
    if proc.returncode != 0 or missing:
        errors = ["accounting-privacy tests failed"]
        errors.extend(f"missing passing test {name}" for name in missing)
        return Report("logs", errors)
    return Report("logs")


CHECKERS = {
    "data-files": check_data_files,
    "private": check_private,
    "denylist": check_denylist,
    "account-numbers": check_account_numbers,
    "logs": check_logs,
}


def resolve_root(arg: Path | None) -> Path:
    if arg is not None:
        return arg.resolve()
    proc = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        check=False,
        capture_output=True,
        text=True,
    )
    if proc.returncode != 0:
        raise SystemExit("privacy: not inside a git repository (pass --root)")
    return Path(proc.stdout.strip())


def cmd_check(root: Path, guard: str | None) -> int:
    selected = (guard,) if guard else GUARDS
    worst = 0
    for name in selected:
        worst = max(worst, emit(CHECKERS[name](root)))
    return worst


class TempRepo:
    def __init__(self) -> None:
        self.path = Path(tempfile.mkdtemp(prefix="ya-privacy-"))
        subprocess.run(["git", "init", "-q", "-b", "main"], cwd=self.path, check=True)
        hooks = self.path / ".git" / "no-hooks"
        hooks.mkdir()
        exclude = self.path / ".git" / "empty-exclude"
        exclude.write_text("", encoding="utf-8")
        subprocess.run(
            ["git", "config", "core.hooksPath", str(hooks)],
            cwd=self.path,
            check=True,
        )
        subprocess.run(
            ["git", "config", "core.excludesFile", str(exclude)],
            cwd=self.path,
            check=True,
        )

    def write_bytes(self, rel: str, data: bytes) -> None:
        path = self.path / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)

    def write(self, rel: str, text: str) -> None:
        self.write_bytes(rel, text.encode("utf-8"))

    def add_force(self, *rels: str) -> None:
        subprocess.run(["git", "add", "-f", "--", *rels], cwd=self.path, check=True)

    def add_all(self) -> None:
        subprocess.run(["git", "add", "-A"], cwd=self.path, check=True)

    def close(self) -> None:
        shutil.rmtree(self.path, ignore_errors=True)

    def __enter__(self) -> TempRepo:
        return self

    def __exit__(self, *_args: object) -> None:
        self.close()


def expect(cond: bool, message: str, report: Report | None = None) -> None:
    if cond:
        return
    if report is not None:
        print(
            f"report {report.name}: config={report.config_error!r} errors={report.errors!r}",
            file=sys.stderr,
        )
    raise SystemExit(f"privacy test failed: {message}")


def run_cli(root: Path, *args: str, env: dict[str, str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(SCRIPT), *args, "--root", str(root)],
        check=False,
        capture_output=True,
        text=True,
        env=env,
    )


def env_without_denylist() -> dict[str, str]:
    env = os.environ.copy()
    env.pop("YA_DENYLIST", None)
    env.pop("YA_DENYLIST_FILE", None)
    return env


def seed_denylist(repo: TempRepo, root: Path) -> None:
    repo.write(HASH_FILE, (root / HASH_FILE).read_text(encoding="utf-8"))
    repo.write(PLACEHOLDER_FILE, (root / PLACEHOLDER_FILE).read_text(encoding="utf-8"))


def first_placeholder(root: Path) -> str:
    terms = term_lines(root / PLACEHOLDER_FILE)
    if not terms:
        raise SystemExit("privacy test failed: placeholder file has no term")
    return terms[0]


def test_data_files(root: Path) -> None:
    counted = REAL_DATA_EXTENSIONS | {".json"}
    expect(REQUIRED_DATA_EXTENSIONS <= counted, "data extension list shrunk")
    expect(".tsv" not in counted and ".xlsx" not in counted, "non-data extensions must stay out of the placement guard")
    with TempRepo() as repo:
        rels = []
        for suffix in sorted(REQUIRED_DATA_EXTENSIONS):
            rel = f"exports/planted{suffix}"
            repo.write_bytes(rel, b"# synthetic\n<!-- synthetic -->\n% synthetic\n")
            rels.append(rel)
        repo.add_force(*rels)
        report = check_data_files(repo.path)
        expect(report.code() == 1, "data files outside fixtures/ must fail", report)
        expect(
            len(report.errors) == len(REQUIRED_DATA_EXTENSIONS),
            "each outside data file must be reported",
            report,
        )
        for rel in rels:
            expect(any(rel in err and "outside" in err for err in report.errors), f"missing {rel}", report)

    with TempRepo() as repo:
        missing = {
            "fixtures/planted.csv": b"Date,Merchant\n2025-10-01,Acme Corp\n",
            "fixtures/planted.ofx": b"<OFX></OFX>\n",
            "fixtures/planted.qfx": b"<OFX></OFX>\n",
            "fixtures/planted.json": b"{}\n",
            "fixtures/planted.sqlite": b"SQLite format 3\x00no marker",
            "fixtures/planted.db": b"no marker",
            "fixtures/planted.pdf": b"%PDF-1.4\n1 0 obj\n",
        }
        for rel, data in missing.items():
            repo.write_bytes(rel, data)
        repo.add_force(*missing)
        report = check_data_files(repo.path)
        expect(report.code() == 1, "fixtures without a synthetic marker must fail", report)
        for rel in missing:
            expect(any(rel in err and "marker" in err for err in report.errors), f"missing {rel}", report)

    with TempRepo() as repo:
        good = {
            "fixtures/planted.csv": b"# synthetic\nDate,Merchant\n2025-10-01,Acme Corp\n",
            "fixtures/planted.ofx": b"<!-- synthetic -->\n<OFX></OFX>\n",
            "fixtures/planted.qfx": b"# synthetic\n<OFX></OFX>\n",
            "fixtures/planted.json": b"# synthetic\n{}\n",
            "fixtures/planted.pdf": b"%PDF-1.4\n% synthetic\n",
            "fixtures/planted.sqlite": b"SQLite format 3\x00no marker",
            "fixtures/planted.sqlite.synthetic": b"# synthetic\n",
            "fixtures/planted.db": b"no marker",
            "fixtures/planted.db.synthetic": b"# synthetic\n",
        }
        for rel, data in good.items():
            repo.write_bytes(rel, data)
        repo.add_force(*good)
        report = check_data_files(repo.path)
        expect(report.code() == 0, "marked synthetic fixtures must pass", report)

    with TempRepo() as repo:
        manifests = {
            "package.json": b'{"name":"prototype","version":"1.0.0"}\n',
            "package-lock.json": b'{"name":"prototype","lockfileVersion":3}\n',
            "tsconfig.json": b'{"compilerOptions":{"strict":true}}\n',
            "prototypes/b-triage/package.json": b'{"name":"triage"}\n',
            "prototypes/b-triage/package-lock.json": b'{"name":"triage","lockfileVersion":3}\n',
            "prototypes/b-triage/tsconfig.json": b'{"files":[]}\n',
            "app/settings.json": b'{"port":5171}\n',
        }
        for rel, data in manifests.items():
            repo.write_bytes(rel, data)
        repo.add_force(*manifests)
        report = check_data_files(repo.path)
        expect(report.code() == 0, "package manifests and unmarked json must pass", report)

    with TempRepo() as repo:
        repo.write("notes/export.json", '{"_synthetic": true, "payee": "Acme Corp"}\n')
        repo.add_force("notes/export.json")
        report = check_data_files(repo.path)
        expect(report.code() == 1, "json with a synthetic marker outside fixtures/ must fail", report)
        expect(any("notes/export.json" in err and "outside" in err for err in report.errors), "marked json must be named", report)

    with TempRepo() as repo:
        good = {
            "prototypes/c-journal/fixtures/planted.csv": b"# synthetic\nDate,Merchant\n2025-10-01,Acme Corp\n",
            "prototypes/c-journal/fixtures/books.json": b'{"_synthetic": true, "name": "Acme Corp"}\n',
            "prototypes/c-journal/fixtures/sidecar.json": b'{"name": "Acme Corp"}\n',
            "prototypes/c-journal/fixtures/sidecar.json.synthetic": b"# synthetic\n",
        }
        for rel, data in good.items():
            repo.write_bytes(rel, data)
        repo.add_force(*good)
        report = check_data_files(repo.path)
        expect(report.code() == 0, "per-prototype fixtures with a marker must pass", report)

    with TempRepo() as repo:
        bad = {
            "prototypes/c-journal/fixtures/plain.json": b'{"name": "Acme Corp"}\n',
            "prototypes/c-journal/fixtures/nested.json": b'{"inner": {"_synthetic": true}}\n',
            "prototypes/c-journal/data/rows.csv": b"# synthetic\nDate,Merchant\n2025-10-01,Acme Corp\n",
        }
        for rel, data in bad.items():
            repo.write_bytes(rel, data)
        repo.add_force(*bad)
        report = check_data_files(repo.path)
        expect(report.code() == 1, "unmarked prototype fixtures and data outside fixtures/ must fail", report)
        for rel in bad:
            expect(any(rel in err for err in report.errors), f"missing {rel}", report)

    with TempRepo() as repo:
        repo.write_bytes("exports/notes.tsv", b"# synthetic\nAcme Corp\n")
        repo.write_bytes("exports/sheet.xlsx", b"# synthetic\n")
        repo.add_force("exports/notes.tsv", "exports/sheet.xlsx")
        report = check_data_files(repo.path)
        expect(report.code() == 0, "tsv and xlsx are not data files for this guard", report)

    report = check_data_files(root)
    expect(report.code() == 0, "clean tree must pass data-files", report)


def test_private(root: Path) -> None:
    with TempRepo() as repo:
        repo.write("private/rows.csv", "# synthetic\nAcme Corp\n")
        repo.add_force("private/rows.csv")
        report = check_private(repo.path)
        expect(report.code() == 1, "private/ without a gitignore rule must fail", report)

    with TempRepo() as repo:
        repo.write(".gitignore", "private/\n")
        repo.write("private/rows.csv", "# synthetic\nAcme Corp\n")
        repo.add_all()
        repo.add_force("private/rows.csv")
        report = check_private(repo.path)
        expect(report.code() == 1, "a tracked file under private/ must fail", report)
        expect(
            any("tracked file under private/" in err for err in report.errors),
            "tracked private file must be named",
            report,
        )

    with TempRepo() as repo:
        repo.write(".gitignore", "private/\n")
        repo.write("README.md", "Acme Corp placeholder\n")
        repo.write("private/rows.csv", "# synthetic\nAcme Corp\n")
        repo.add_all()
        report = check_private(repo.path)
        expect(report.code() == 0, "gitignored private/ must pass", report)
        files = git_files(repo.path)
        expect("private/rows.csv" not in files, "private file must stay untracked")

    report = check_private(root)
    expect(report.code() == 0, "clean tree must pass private", report)


def test_denylist(root: Path) -> None:
    term = first_placeholder(root)
    with TempRepo() as repo:
        seed_denylist(repo, root)
        repo.write("notes/planted.txt", f"employer {term} end\n")
        repo.add_force(HASH_FILE, PLACEHOLDER_FILE, "notes/planted.txt")
        report = check_denylist(repo.path, env={})
        expect(report.code() == 1, "placeholder term in a tracked file must fail", report)
        expect(all(term not in err for err in report.errors), "errors must not echo the term", report)
        proc = run_cli(repo.path, "denylist", env=env_without_denylist())
        expect(proc.returncode == 1, "denylist cli must fail on a planted term")
        combined = proc.stdout + proc.stderr
        expect(term not in combined, "denylist cli must not print the term")
        expect("not printed" in combined, "denylist cli must say terms are not printed")

    with TempRepo() as repo:
        seed_denylist(repo, root)
        repo.write("notes/ok.txt", "employer Acme Corp payroll\n")
        repo.add_force(HASH_FILE, PLACEHOLDER_FILE, "notes/ok.txt")
        report = check_denylist(repo.path, env={})
        expect(report.code() == 0, "Acme Corp must pass the denylist", report)

    env_term = "EnvSyntheticTerm"
    with TempRepo() as repo:
        seed_denylist(repo, root)
        repo.write("notes/env.txt", f"saw {env_term} here\n")
        repo.add_force(HASH_FILE, PLACEHOLDER_FILE, "notes/env.txt")
        bad = check_denylist(repo.path, env={"YA_DENYLIST": env_term})
        expect(bad.code() == 1, "YA_DENYLIST term in a tracked file must fail", bad)
        expect(all(env_term not in err for err in bad.errors), "errors must not echo the env term", bad)
        good = check_denylist(repo.path, env={})
        expect(good.code() == 0, "the same tree must pass without YA_DENYLIST", good)
        cli_env = env_without_denylist()
        cli_env["YA_DENYLIST"] = env_term
        proc = run_cli(repo.path, "denylist", env=cli_env)
        expect(proc.returncode == 1, "cli must honor YA_DENYLIST")
        expect(env_term not in proc.stdout + proc.stderr, "cli must not print the env term")

    local_term = "LocalSyntheticTerm"
    with TempRepo() as repo:
        seed_denylist(repo, root)
        repo.write(".gitignore", "private/\n")
        repo.write(LOCAL_DENYLIST, local_term + "\n")
        repo.write("notes/ok.txt", "Acme Corp only\n")
        repo.add_all()
        good = check_denylist(repo.path, env={})
        expect(good.code() == 0, "a local denylist alone must not fail a clean tree", good)
        repo.write("notes/bad.txt", f"hello {local_term}\n")
        repo.add_force("notes/bad.txt")
        bad = check_denylist(repo.path, env={})
        expect(bad.code() == 1, "a local denylist term in a tracked file must fail", bad)
        expect(all(local_term not in err for err in bad.errors), "errors must not echo the local term", bad)

    hash_token_text = "SyntheticHashToken"
    digest = hashlib.sha256(hash_token_text.lower().encode("ascii")).hexdigest()
    with TempRepo() as repo:
        repo.write(HASH_FILE, digest + "\n")
        repo.write(PLACEHOLDER_FILE, (root / PLACEHOLDER_FILE).read_text(encoding="utf-8"))
        repo.write("notes/hash.txt", f"token {hash_token_text} end\n")
        repo.add_force(HASH_FILE, PLACEHOLDER_FILE, "notes/hash.txt")
        report = check_denylist(repo.path, env={})
        expect(report.code() == 1, "a hashed token must fail", report)
        blob = "\n".join(report.errors)
        expect(hash_token_text not in blob and hash_token_text.lower() not in blob, "hash hit must not print the token", report)
        expect(any(digest in err for err in report.errors), "hash hit must report the digest", report)

    report = check_denylist(root)
    expect(report.code() == 0, "clean tree must pass denylist", report)


def test_account_numbers(root: Path) -> None:
    with TempRepo() as repo:
        repo.write(
            "fixtures/planted.csv",
            "# synthetic\naccount,****1234\naccount,...1234\naccount,xxxx1234\n",
        )
        repo.add_force("fixtures/planted.csv")
        report = check_account_numbers(repo.path)
        expect(report.code() == 0, "masked last-4 in a fixture must pass", report)

    with TempRepo() as repo:
        repo.write("fixtures/planted.csv", "# synthetic\naccount,123456789012\n")
        repo.add_force("fixtures/planted.csv")
        report = check_account_numbers(repo.path)
        expect(report.code() == 1, "a long digit run in a fixture must fail", report)
        expect(all("123456789012" not in err for err in report.errors), "must not print the digits", report)

    with TempRepo() as repo:
        repo.write("fixtures/amounts.csv", "# synthetic\nUSD 120000\n")
        repo.add_force("fixtures/amounts.csv")
        report = check_account_numbers(repo.path)
        expect(report.code() == 1, "fixtures reject digit runs longer than 4 even on money lines", report)

    with TempRepo() as repo:
        repo.write(
            "docs/accounts.md",
            "\n".join(
                [
                    "masked ****1234",
                    "dots ...1234",
                    "exes xxxx1234",
                    "posted 2026-11-27",
                    "see ADR 0002-0004",
                    "expectation USD 120000",
                    "retry R1, debit 120001",
                    "",
                ]
            ),
        )
        repo.add_force("docs/accounts.md")
        report = check_account_numbers(repo.path)
        expect(report.code() == 0, "masked last-4, dates, and minor units in docs must pass", report)

    with TempRepo() as repo:
        repo.write("docs/planted.md", "debit 123456789012\n")
        repo.add_force("docs/planted.md")
        report = check_account_numbers(repo.path)
        expect(report.code() == 1, "a long digit run stays a violation on a debit line", report)

    planted_docs = {
        "account 12345\n": "five digits",
        "account 123456789012\n": "twelve digits",
        "account ****12345\n": "mask with more than last-4",
        "grouped 1234-5678-9012\n": "grouped digits",
        "iban ZZ00TEST123456789012\n": "iban shape",
    }
    for body, label in planted_docs.items():
        with TempRepo() as repo:
            repo.write("docs/planted.md", body)
            repo.add_force("docs/planted.md")
            report = check_account_numbers(repo.path)
            expect(report.code() == 1, f"docs must reject {label}", report)

    with TempRepo() as repo:
        repo.write(
            "prototypes/c-journal/fixtures/books.json",
            "\n".join(
                [
                    "{",
                    '  "_synthetic": true,',
                    '  "openingMinor": 120000,',
                    '  "fromAmountMinor": 10050,',
                    '  "amount_minor": "120001"',
                    "}",
                    "",
                ]
            ),
        )
        repo.add_force("prototypes/c-journal/fixtures/books.json")
        report = check_account_numbers(repo.path)
        expect(report.code() == 0, "minor-unit JSON values of at most 6 digits must pass", report)

    with TempRepo() as repo:
        repo.write(
            "prototypes/c-journal/fixtures/books.json",
            '{"_synthetic": true, "openingMinor": 123456789012}\n',
        )
        repo.add_force("prototypes/c-journal/fixtures/books.json")
        report = check_account_numbers(repo.path)
        expect(report.code() == 1, "a long digit run in a minor JSON field must fail", report)
        expect(all("123456789012" not in err for err in report.errors), "must not print the digits", report)

    with TempRepo() as repo:
        repo.write(
            "prototypes/b-triage/src/accounts.ts",
            'export const account = "123456789012";\nexport const openingMinor = 120000;\n',
        )
        repo.add_force("prototypes/b-triage/src/accounts.ts")
        report = check_account_numbers(repo.path)
        expect(report.code() == 1, "prototypes/ must be scanned for account numbers", report)
        expect(any("prototypes/b-triage/src/accounts.ts" in err for err in report.errors), "prototype path must be named", report)

    with TempRepo() as repo:
        repo.write(
            "prototypes/c-journal/fixtures/books.json",
            '{"minorAccount": 123456, "openingMinor": 100}\n',
        )
        repo.add_force("prototypes/c-journal/fixtures/books.json")
        report = check_account_numbers(repo.path)
        expect(report.code() == 1, "a key that does not end in minor is still an account number", report)

    with TempRepo() as repo:
        repo.write("NOTES.md", "account 123456789012\n")
        repo.add_force("NOTES.md")
        report = check_account_numbers(repo.path)
        expect(report.code() == 1, "markdown outside docs/ must be scanned", report)

    with TempRepo() as repo:
        repo.write("exports/rows.csv", "account,123456789012\n")
        repo.add_force("exports/rows.csv")
        report = check_account_numbers(repo.path)
        expect(report.code() == 1, "tracked text data outside fixtures/ must be scanned", report)

    report = check_account_numbers(root)
    expect(report.code() == 0, "clean tree must pass account-numbers", report)


def write_privacy_crate(repo: TempRepo, lib_rs: str) -> None:
    repo.write(
        "accounting-core/Cargo.toml",
        "\n".join(
            [
                "[workspace]",
                'resolver = "2"',
                'members = ["crates/accounting-privacy"]',
                "",
            ]
        ),
    )
    repo.write(
        "accounting-core/crates/accounting-privacy/Cargo.toml",
        "\n".join(
            [
                "[package]",
                'name = "accounting-privacy"',
                'version = "0.0.0"',
                'edition = "2021"',
                "",
            ]
        ),
    )
    repo.write("accounting-core/crates/accounting-privacy/src/lib.rs", lib_rs)


def test_logs(root: Path) -> None:
    with TempRepo() as repo:
        repo.write("README.md", "Acme Corp placeholder\n")
        repo.add_all()
        report = check_logs(repo.path)
        expect(report.code() == 0, "a tree without the crate must skip logs", report)
        expect(
            report.skipped is not None and "accounting-privacy crate is absent" in report.skipped,
            "skip must name the missing crate",
            report,
        )
        proc = run_cli(repo.path, "check", "logs", env=env_without_denylist())
        expect(proc.returncode == 0, "logs cli must exit 0 when the crate is absent")
        expect("SKIP" in proc.stdout, "logs cli must print SKIP")
        expect("accounting-privacy crate is absent" in proc.stdout, "logs cli must say why it skipped")

    report = check_logs(root, cargo_bin=None)
    expect(report.code() == 0, "missing cargo must skip even when the crate exists", report)
    expect(report.skipped is not None and "cargo is absent" in report.skipped, "skip must name cargo", report)

    with TempRepo() as repo:
        write_privacy_crate(
            repo,
            "\n".join(
                [
                    "#[cfg(test)]",
                    "mod tests {",
                    "    #[test]",
                    "    fn clean_log_redacts_synthetic_transaction() {",
                    '        panic!("planted unredacted log");',
                    "    }",
                    "",
                    "    #[test]",
                    "    fn planted_unredacted_log_fails_the_guard() {}",
                    "}",
                    "",
                ]
            ),
        )
        repo.add_all()
        report = check_logs(repo.path)
        expect(report.code() == 1, "a present crate whose redaction test fails must fail", report)
        expect(report.skipped is None, "a failing redaction test must not skip", report)
        proc = run_cli(repo.path, "check", "logs", env=env_without_denylist())
        expect(proc.returncode == 1, "logs cli must exit 1 when the redaction test fails")
        expect("SKIP" not in proc.stdout, "a failing redaction test must not print SKIP")

    report = check_logs(root)
    expect(report.code() == 0, "clean log redaction tests must pass", report)
    expect(report.skipped is None, "logs must run when the crate and cargo are present", report)
    expect(
        (root / "accounting-core/crates/accounting-privacy/src/lib.rs").is_file(),
        "redacting logger source must exist",
    )


TESTS = {
    "data-files": test_data_files,
    "private": test_private,
    "denylist": test_denylist,
    "account-numbers": test_account_numbers,
    "logs": test_logs,
}


def cmd_test(root: Path, guard: str | None) -> int:
    selected = (guard,) if guard else GUARDS
    for name in selected:
        TESTS[name](root)
        print(f"test {name}: ok")
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="U14 privacy guards")
    sub = parser.add_subparsers(dest="cmd", required=True)

    check = sub.add_parser("check", help="run guards against the tree")
    check.add_argument("guard", nargs="?", choices=GUARDS)
    check.add_argument("--root", type=Path, default=None)

    test = sub.add_parser("test", help="plant synthetic violations and recheck the clean tree")
    test.add_argument("guard", nargs="?", choices=GUARDS)
    test.add_argument("--root", type=Path, default=None)

    denylist = sub.add_parser("denylist", help="run only the denylist guard")
    denylist.add_argument("--root", type=Path, default=None)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    root = resolve_root(args.root)
    if args.cmd == "check":
        return cmd_check(root, args.guard)
    if args.cmd == "test":
        return cmd_test(root, args.guard)
    if args.cmd == "denylist":
        return emit(check_denylist(root))
    raise SystemExit(f"unknown command {args.cmd}")


if __name__ == "__main__":
    sys.exit(main())
