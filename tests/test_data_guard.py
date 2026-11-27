"""Tests for scripts/data_guard.py.

Account-number-like values used here are assembled at runtime so this
file itself stays clearly synthetic.
"""

from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
import unittest
from pathlib import Path
from types import ModuleType

REPO_ROOT = Path(__file__).resolve().parents[1]


def _load() -> ModuleType:
    path = REPO_ROOT / "scripts" / "data_guard.py"
    spec = importlib.util.spec_from_file_location("data_guard", path)
    assert spec and spec.loader
    mod = importlib.util.module_from_spec(spec)
    sys.modules["data_guard"] = mod
    spec.loader.exec_module(mod)
    return mod


guard = _load()

# Five-plus digit runs built at runtime (not a real account).
LONG_ACCT = "9" * 3 + "8" * 3
SHORT_ACCT = "12" + "34"  # last-4 only


def _git(repo: Path, *args: str) -> subprocess.CompletedProcess[bytes]:
    return subprocess.run(
        ["git", *args],
        cwd=repo,
        check=True,
        capture_output=True,
    )


class MarkerTests(unittest.TestCase):
    def test_csv_comment_marker(self) -> None:
        text = "# synthetic\nDate,Amount\n2025-10-01,1.00\n"
        self.assertTrue(guard.has_synthetic_marker("fixtures/x.csv", text))

    def test_csv_requires_comment_line(self) -> None:
        text = "Date,Amount\n2025-10-01,1.00\n"
        self.assertFalse(guard.has_synthetic_marker("fixtures/x.csv", text))
        # The words in a data field are not a marker.
        text = "note,# synthetic in a field\n"
        self.assertFalse(guard.has_synthetic_marker("fixtures/x.csv", text))

    def test_json_synthetic_key(self) -> None:
        text = json.dumps({"_synthetic": True, "rows": []})
        self.assertTrue(guard.has_synthetic_marker("fixtures/x.json", text))
        self.assertFalse(
            guard.has_synthetic_marker(
                "fixtures/x.json", json.dumps({"_synthetic": False, "rows": []})
            )
        )
        self.assertFalse(
            guard.has_synthetic_marker(
                "fixtures/x.json", json.dumps({"rows": []})
            )
        )

    def test_json_leading_comment(self) -> None:
        text = '// # synthetic\n{"rows": []}\n'
        self.assertTrue(guard.has_synthetic_marker("fixtures/x.json", text))
        text = '# synthetic\n{"rows": []}\n'
        self.assertTrue(guard.has_synthetic_marker("fixtures/x.json", text))
        self.assertFalse(
            guard.has_synthetic_marker("fixtures/x.json", '{"rows": []}\n')
        )


class PathTests(unittest.TestCase):
    def test_csv_must_live_under_fixtures(self) -> None:
        self.assertEqual(
            guard.path_problems("notes/export.csv"),
            ["CSV files must live under a fixtures/ directory"],
        )
        self.assertEqual(guard.path_problems("fixtures/oct.csv"), [])
        self.assertEqual(guard.path_problems("tests/fixtures/oct.csv"), [])
        self.assertEqual(guard.path_problems("accounting-core/fixtures/x.csv"), [])
        self.assertEqual(
            guard.path_problems("fixtures.csv"),
            ["CSV files must live under a fixtures/ directory"],
        )

    def test_json_allowed_outside_fixtures(self) -> None:
        self.assertEqual(guard.path_problems("docs/example.json"), [])


class AccountNumberTests(unittest.TestCase):
    def test_long_digit_run_detected(self) -> None:
        self.assertTrue(guard.line_problems(f"account {LONG_ACCT}"))
        # Three grouped parts, or a part longer than 4 digits, look like accounts.
        self.assertTrue(guard.line_problems("-".join([LONG_ACCT[:2], LONG_ACCT[2:4], LONG_ACCT[4:]])))
        self.assertTrue(guard.line_problems(" ".join([LONG_ACCT[:2], LONG_ACCT[2:4], LONG_ACCT[4:]])))
        self.assertTrue(guard.line_problems(f"{LONG_ACCT[:5]}-12"))

    def test_last_four_allowed(self) -> None:
        self.assertEqual(guard.line_problems(f"account ****{SHORT_ACCT}"), [])
        self.assertEqual(guard.line_problems(SHORT_ACCT), [])

    def test_dates_not_flagged(self) -> None:
        for line in (
            "date 2025-10-01",
            "posted 2025-10-01T13:45:00Z",
            "us 10/01/2025",
            "compact 20251001",
        ):
            with self.subTest(line=line):
                self.assertEqual(guard.line_problems(line), [])

    def test_amounts_not_flagged(self) -> None:
        minor = "12" + "0" * 4  # six-digit minor units, assembled at runtime
        rate = "1." + "185" + "0000"
        for line in (
            "total $11385.00 USD",
            "amount 1,234.56",
            "cents 17985.00",
            "Money::new(10000_00, \"USD\")",
            "1_000_000",
            f"expectation USD {minor}",
            f"debit {minor}",
            f"{minor} minor units, USD",
            f"VALUES('${{P}}EV20','USD',{minor})",
            f"amount_minor) VALUES('x','USD','debit',{minor})",
            f"r = {minor} / {minor} x 10^(2 - 2) = {rate}",
            f"USD {minor} = {minor}",
            f"EUR|{minor}",
            f"shown as **{rate} USD per EUR**",
            f"r would be {rate}.",
            "0." + "84" + "388" + "185" + "...",
            "| Credit | EUR 1,000.00 | " + "1" + "0" * 5 + " |",
        ):
            with self.subTest(line=line):
                self.assertEqual(guard.line_problems(line), [])

    def test_versions_not_flagged(self) -> None:
        self.assertEqual(guard.line_problems('version = "1.2.345"'), [])
        self.assertEqual(guard.line_problems(" rusqlite 0.32.1 "), [])

    def test_alphanumeric_tokens_and_hashes_not_flagged(self) -> None:
        # Letters on either side, as in lockfile checksums and hex ids.
        self.assertEqual(
            guard.line_problems(
                'checksum = "5a15f179cd60c4584b8a8c596927aadc462e27f2ca70c04e0071964a73ba7a75"'
            ),
            [],
        )
        self.assertEqual(guard.line_problems("sha ab" + LONG_ACCT + "cd"), [])
        self.assertEqual(guard.line_problems("je_0000000a"), [])
        self.assertEqual(guard.line_problems("token" + LONG_ACCT), [])

        self.assertEqual(guard.line_problems("Entry [je_00000016]"), [])
        self.assertEqual(guard.line_problems("docs/adr/0001-0008"), [])

    def test_findings_omit_matched_text(self) -> None:
        reports = list(guard.scan_lines("f.csv", [(7, f"secret {LONG_ACCT}")]))
        self.assertTrue(reports)
        joined = "\n".join(reports)
        self.assertIn("f.csv:7:", joined)
        self.assertNotIn(LONG_ACCT, joined)
        self.assertNotIn("secret", joined)


class InspectFileTests(unittest.TestCase):
    def test_marked_fixture_csv_ok(self) -> None:
        data = b"# synthetic\nDate,Account,Amount\n2025-10-01,****1234,1.00\n"
        self.assertEqual(guard.inspect_file("fixtures/ok.csv", data), [])

    def test_unmarked_csv_fails(self) -> None:
        data = b"Date,Amount\n2025-10-01,1.00\n"
        findings = guard.inspect_file("fixtures/bad.csv", data)
        self.assertTrue(any("missing synthetic marker" in f for f in findings))
        self.assertTrue(all(LONG_ACCT not in f and "1.00" not in f for f in findings))

    def test_csv_outside_fixtures_fails(self) -> None:
        data = b"# synthetic\nDate,Amount\n2025-10-01,1.00\n"
        findings = guard.inspect_file("docs/out.csv", data)
        self.assertTrue(any("fixtures/" in f for f in findings))

    def test_unmarked_json_fails(self) -> None:
        data = b'{"rows": []}\n'
        findings = guard.inspect_file("fixtures/bad.json", data)
        self.assertTrue(any("missing synthetic marker" in f for f in findings))

    def test_marked_json_ok(self) -> None:
        data = b'{"_synthetic": true, "rows": []}\n'
        self.assertEqual(guard.inspect_file("docs/ok.json", data), [])


class AccountScanScopeTests(unittest.TestCase):
    def test_python_source_is_not_an_account_scan_target(self) -> None:
        self.assertFalse(guard.account_scan_target("scripts/privacy_guard.py"))
        self.assertFalse(guard.account_scan_target("accounting-core/crates/foo/src/lib.rs"))

    def test_docs_and_data_are_account_scan_targets(self) -> None:
        self.assertTrue(guard.account_scan_target("CONTRIBUTING.md"))
        self.assertTrue(guard.account_scan_target("docs/schema-v2-smoke.sh"))
        self.assertTrue(guard.account_scan_target("fixtures/monarch/october-2025-synthetic.csv"))
        self.assertTrue(guard.account_scan_target("prototypes/demo/app.ts"))

    def test_python_source_with_digit_run_is_not_flagged(self) -> None:
        data = f"repo.write('fixtures/x.csv', 'account,{LONG_ACCT}')\n".encode()
        self.assertEqual(guard.inspect_file("scripts/privacy_guard.py", data), [])


class GitIntegrationTests(unittest.TestCase):

    def _init_repo(self, tmp: Path) -> Path:
        repo = tmp / "r"
        repo.mkdir()
        _git(repo, "init", "-q")
        _git(repo, "config", "user.name", "Test")
        _git(repo, "config", "user.email", "test@example.com")
        return repo

    def test_tracked_all_and_staged(self) -> None:
        import tempfile

        with tempfile.TemporaryDirectory() as raw:
            repo = self._init_repo(Path(raw))
            (repo / "fixtures").mkdir()
            (repo / "fixtures" / "ok.csv").write_text(
                "# synthetic\nDate,Amount\n2025-10-01,1.00\n", encoding="utf-8"
            )
            _git(repo, "add", "fixtures/ok.csv")
            _git(repo, "commit", "-q", "-m", "ok")
            self.assertEqual(guard.tracked_findings(repo), [])

            (repo / "fixtures" / "bad.csv").write_text(
                "Date,Amount\n2025-10-01,1.00\n", encoding="utf-8"
            )
            _git(repo, "add", "fixtures/bad.csv")
            staged = guard.staged_findings(repo)
            self.assertTrue(any("missing synthetic marker" in f for f in staged))
            self.assertTrue(all("1.00" not in f for f in staged))


if __name__ == "__main__":
    unittest.main()
