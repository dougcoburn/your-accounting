# Contributing

## Denylist

This repository must not contain certain employer or personal names in tracked files.

`scripts/check-denylist.sh` scans tracked text files for word tokens whose SHA-256 digest (of the lowercase token) appears in `scripts/denylist.sha256`. The hash file stores only digests, so the forbidden terms themselves are not in the repo.

Run the check locally before opening a pull request:

```bash
scripts/check-denylist.sh
```

There is no GitHub Actions workflow in this repository yet. When one is added, include a step that runs `bash scripts/check-denylist.sh`. A starter workflow:

```yaml
name: denylist
on:
  push:
  pull_request:
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Check denylist
        run: bash scripts/check-denylist.sh
```

To add a term, hash the lowercase word and append the hex digest to `scripts/denylist.sha256`. Do not commit the term in plaintext:

```bash
printf '%s' "$word" | sha256sum
```
