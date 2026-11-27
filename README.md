# your-accounting

Personal accounting software. Design notes live under `docs/`. The current kernel is in `accounting-core/`.

The month-1 implementation plan, which is the month-1 spec for now, is in [`docs/MONTH1_IMPLEMENTATION_PLAN.md`](docs/MONTH1_IMPLEMENTATION_PLAN.md). It is still in review. U17 already landed in #31.

See [CONTRIBUTING.md](CONTRIBUTING.md) for the privacy guards. `bash scripts/check-privacy.sh` checks synthetic fixtures, `private/`, the denylist, account numbers, and redacted logs. Acme Corp is the employer placeholder.
