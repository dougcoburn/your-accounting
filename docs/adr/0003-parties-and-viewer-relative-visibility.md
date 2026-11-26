# ADR 0003: One Party concept; visibility is viewer-relative

Status: Proposed (revised 2026-11-20). Changes Spec v0.2's transparent/opaque entity model.

## Context
v0.2 distinguishes transparent entities, opaque entities and parties. Mechanically an opaque entity and a vendor are the same: someone outside the books who can hold obligations. v0.2 also makes visibility an entity property, but Esther may later share some books (Household, Dragonfly) while personal books stay private.

## Decision
- **Party**: anyone outside a viewer's books (merchants, employers, people). Any party can be creditor or debtor on an obligation. "Vendor" vs "person/organization" is UI labeling.
- **Entity**: a set of books, in its own database (ADR 0004). Users are **granted access** to entities.
- Each entity carries a **stable party identity** (`entity.party_id`, same globally unique id in every database). For a viewer without access, that id simply resolves to a party.
- A user without access to an entity cannot write into its books. To them it is a vendor like any other; reimbursement requests go out of band and the other entity records them when received.
- Month 1: entities Doug, Household, Dragonfly; one user (Doug) with a single access grant. Esther and Acme Corp are parties. Esther joining later is an access grant, with no re-pointing of history.

## Consequences
Removes `entities.visibility` ('opaque'), `parties.opaque_entity_id` and the `account_owner_valid` trigger from the v0.2 schema; adds a user-to-entity grant table (migration + prior-version fixture test per SPEC §2). Viewer-relative multi-user UI is deferred past month 1.
