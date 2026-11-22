import { latestFigures, sameFigures, type FleetFacts } from "@/lib/turns/run-summary";
import type { Entry, FleetStreamSnapshot } from "./fleet-stream-entry";
import { isPreviewCommand } from "./fleet-preview-command";
import { mergeFacts } from "./fleet-stream-facts";
import type { FleetTurn } from "./fleet-stream-row";

// An entry's one write path: its rows, the facts a frame spoke about the fleet,
// and any other snapshot patch. Listeners hear of a write once, and only when
// it changed something. Split from the registry, which owns the map and the
// EventSource lifecycle and calls through here for every mutation.

function notify(entry: Entry): void {
  for (const l of entry.listeners) l();
}

export function patchSnapshot(entry: Entry, patch: Partial<FleetStreamSnapshot>): void {
  entry.snapshot = { ...entry.snapshot, ...patch };
  notify(entry);
}

export function setTurns(
  entry: Entry,
  next: (prev: FleetTurn[]) => FleetTurn[],
  spoken: Partial<FleetFacts> = {},
  replyTurnId?: string,
): void {
  // The one choke point every mutation flows through: the strip's
  // `latest` lives here once, a completion's facts fold into the same write, and
  // a frame that changed nothing (a duplicate, a malformed one) notifies no one.
  const turns = next(entry.snapshot.turns);
  const facts = spokenFacts(entry, spoken);
  if (turns === entry.snapshot.turns && facts.fleet === undefined) return;
  const latest = latestFigures(turns);
  const replying = replyTurnId === undefined ? undefined : entry.snapshot.turns.find(turn => turn.id === replyTurnId);
  // A preview link can arrive in a reply after its command records the URL.
  const textOnly = replying && !replying.tools?.some(tool => tool.name === "exec_command" && isPreviewCommand(tool.args?.cmd));
  entry.snapshot = {
    ...entry.snapshot,
    ...facts,
    turns,
    workTurns: textOnly ? entry.snapshot.workTurns : turns,
    latest: sameFigures(latest, entry.snapshot.latest) ? entry.snapshot.latest : latest,
  };
  notify(entry);
}

// What a FRAME said about the fleet itself, as a snapshot patch: the merged
// facts and the advanced sequence, or nothing when the frame restated what
// the snapshot already held.
function spokenFacts(entry: Entry, patch: Partial<FleetFacts>): Partial<FleetStreamSnapshot> {
  const fleet = mergeFacts(entry.snapshot.fleet, patch);
  if (fleet === entry.snapshot.fleet) return {};
  return { fleet, factsSeq: entry.snapshot.factsSeq + 1 };
}

// A different gate can replace the pending one without changing the count.
export function patchGateFacts(entry: Entry, patch: Partial<FleetFacts>): void {
  const spoken = spokenFacts(entry, patch);
  patchSnapshot(entry, {...spoken, approvalsSeq: entry.snapshot.approvalsSeq + 1});
}
