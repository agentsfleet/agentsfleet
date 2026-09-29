import { latestFigures, sameFigures, type FleetFacts } from "@/lib/events/run-summary";
import { capEvents } from "./fleet-stream-cap";
import type { Entry, FleetStreamSnapshot } from "./fleet-stream-entry";
import { mergeFacts } from "./fleet-stream-facts";
import type { FleetEvent } from "./fleet-stream-row";

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

export function setEvents(
  entry: Entry,
  next: (prev: FleetEvent[]) => FleetEvent[],
  spoken: Partial<FleetFacts> = {},
): void {
  // The one choke point every mutation flows through: the cap and the strip's
  // `latest` live here once, a completion's facts fold into the same write, and
  // a frame that changed nothing (a duplicate, a malformed one) notifies no one.
  const events = capEvents(next(entry.snapshot.events));
  const facts = spokenFacts(entry, spoken);
  if (events === entry.snapshot.events && facts.fleet === undefined) return;
  const latest = latestFigures(events);
  entry.snapshot = {
    ...entry.snapshot,
    ...facts,
    events,
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

// A gate frame moves the count and no row. Nothing is notified when nothing
// changed, so a frame restating the count does not wake anyone.
export function patchSpokenFacts(entry: Entry, patch: Partial<FleetFacts>): void {
  const spoken = spokenFacts(entry, patch);
  if (spoken.fleet !== undefined) patchSnapshot(entry, spoken);
}
