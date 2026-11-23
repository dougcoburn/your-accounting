import { addMinor } from "./money.ts";
import { isActive, naturalBalance, naturalDelta } from "./query.ts";
import type { JournalEvent, Ledger, LedgerAccount, Observation, Side } from "./types.ts";

export interface ReconLine {
  eventId: string;
  observationId: string;
  memo: string;
  occurredOn: string;
  postedOn: string | null;
  naturalMinor: number;
  side: Side;
  amountMinor: number;
}

export interface Reconciliation {
  account: LedgerAccount;
  cutoff: string;
  occurredMinor: number;
  bookNotStatement: ReconLine[];
  statementNotBook: ReconLine[];
  unknownPosted: ReconLine[];
  explainedMinor: number;
  unposted: Observation[];
}

function lineFor(event: JournalEvent, observation: Observation, account: LedgerAccount, side: Side, amountMinor: number): ReconLine {
  return {
    eventId: event.id,
    observationId: observation.id,
    memo: event.memo,
    occurredOn: event.effectiveOn,
    postedOn: observation.postedOn,
    naturalMinor: naturalDelta(account, side, amountMinor),
    side,
    amountMinor,
  };
}

function observationFor(ledger: Ledger, event: JournalEvent, account: LedgerAccount): Observation | null {
  const linked = ledger.observations.filter((observation) => event.observationIds.includes(observation.id));
  return linked.find((observation) => observation.accountName === account.name) ?? linked[0] ?? null;
}

export function reconcileAccount(ledger: Ledger, accountId: string, cutoff: string): Reconciliation | null {
  const account = ledger.accounts.find((item) => item.id === accountId);
  if (!account) {
    return null;
  }
  const bookNotStatement: ReconLine[] = [];
  const statementNotBook: ReconLine[] = [];
  const unknownPosted: ReconLine[] = [];
  for (const event of ledger.events) {
    if (!isActive(event)) {
      continue;
    }
    for (const posting of event.postings) {
      if (posting.accountId !== account.id) {
        continue;
      }
      const observation = observationFor(ledger, event, account);
      if (!observation) {
        continue;
      }
      const line = lineFor(event, observation, account, posting.side, posting.amountMinor);
      if (observation.postedOn === null) {
        unknownPosted.push(line);
        continue;
      }
      const occurred = event.effectiveOn <= cutoff;
      const posted = observation.postedOn <= cutoff;
      if (occurred && !posted) {
        bookNotStatement.push(line);
      } else if (!occurred && posted) {
        statementNotBook.push(line);
      }
    }
  }
  const byDate = (left: ReconLine, right: ReconLine) => {
    if (left.occurredOn !== right.occurredOn) {
      return left.occurredOn < right.occurredOn ? -1 : 1;
    }
    return left.observationId < right.observationId ? -1 : left.observationId > right.observationId ? 1 : 0;
  };
  bookNotStatement.sort(byDate);
  statementNotBook.sort(byDate);
  unknownPosted.sort(byDate);
  const occurredMinor = naturalBalance(ledger.events, account, cutoff);
  const less = bookNotStatement.reduce((sum, line) => addMinor(sum, line.naturalMinor), 0);
  const plus = statementNotBook.reduce((sum, line) => addMinor(sum, line.naturalMinor), 0);
  return {
    account,
    cutoff,
    occurredMinor,
    bookNotStatement,
    statementNotBook,
    unknownPosted,
    explainedMinor: addMinor(addMinor(occurredMinor, -less), plus),
    unposted: ledger.unposted
      .map((item) => ledger.observations.find((observation) => observation.id === item.observationId))
      .filter((observation): observation is Observation => observation !== undefined && observation.accountName === account.name),
  };
}
