# Unit brief template

Use this template for every unit brief and every subagent brief (proposed with [ADR 0010](../adr/0010-bounded-contexts-small-agent-context.md)). Keep the filled brief to one screen. Cite § anchors instead of pasting their text.

```markdown
## Unit <Uxx>: <one-line change>

- Base: origin/main at <sha>. Branch: <name>. Open a PR only; never merge.
- Context: <ledger-core | observation-import | settlement | recipes | cross-entity | fx | doctor | app-shell | read-models | ui>
- Interface page: docs/contexts/<context>/INTERFACE.md

### Load exactly
- <file> §<n>, §<m>
- GLOSSARY terms: <term>, <term>  (or the context page's term list)
- Schema: <table>, <view>

### Do not load
- <files or §s that look relevant but are not>; never /workspace/ya-private-data or any attachments folder

### Already decided (links, not restated)
- <ADR / PR / § link>: <one line>

### Done when
- <acceptance tests / ATs>
- scripts/check-denylist.sh passes; synthetic data only; no employer name (Acme Corp)

### Stop and ask (one batch, in the PR body)
- Only one-way-door questions (schema shape, invariants, stored data). Pick and state defaults for the rest.
```

## Example: U21 (`doctor` runner and the ledger-core checks)

```markdown
## Unit U21: doctor runner + ledger-core checks

- Base: origin/main at <sha>. Context: doctor (runner) + ledger-core (checks).
- Interface page: docs/contexts/doctor/INTERFACE.md

### Load exactly
- docs/KERNEL_INTERFACE_SPEC.md §3.12, §3.13, §4.2, §5, §8.9, §8.13
- docs/design/bounded-contexts.md §6
- Schema: kernel_state, commits, doctor_unsealed_rows, doctor_unbalanced_events, doctor_superseded_drift

### Do not load
- ADR 0009, ADR 0004 worked examples, KIS §3.7–§3.9, §4.3 (beyond the codes in §4.2)

### Done when
- Planted drift is detected for each of the six ledger-core rows in bounded-contexts §6.2
- Warning/info findings never change the exit status
```
