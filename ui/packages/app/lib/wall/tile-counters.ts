import { AGENTSFLEET_EVENT_STATUS, type FleetEvent } from "@/lib/streaming/fleet-stream-row";

/**
 * The tile footer, advanced by the frames already in the browser.
 *
 * # Nothing is fetched
 *
 * The workspace stream is already open and already carries each settled row's
 * own charge (`costNanos` — the daemon's summed telemetry, not token
 * arithmetic). So the numbers move with no request at all.
 *
 * # Why the streamed rows are a pure delta
 *
 * A wall tile's list is written only by the live frames and the reconnect
 * backfill (`useWorkspaceStream`'s `#eventsByFleet`); nothing seeds it from the
 * server render. Every row in it therefore arrived AFTER the base counters were
 * rendered, so adding them needs no timestamp watermark, and `applyLiveFrame` /
 * `mergeBackfill` merge by row identity, so the list is its own de-duplication.
 * When the base is replaced from the server, the caller drops the rows it has
 * already absorbed — an explicit hand-off rather than a guess about clocks.
 * `fleets.updated_at` is NOT usable as that watermark: the counters live in
 * `core.fleet_activity_counters`, and a run that moves only counters never
 * touches the fleet row's own timestamp.
 *
 * # What the two counters actually count
 *
 * Read from the triggers, because they do not agree. `events_processed` is
 * advanced `AFTER INSERT ON core.fleet_events` (`schema/890`), so it counts a
 * row being RECEIVED, not finishing — counting completions would under-report
 * a fleet with work in flight. `budget_used_nanos` is advanced from
 * `billing.usage_ledger`, whose comment warns one event may update it "forty
 * times"; a frame's `costNanos` is that event's summed total, landing once.
 *
 * # The floor, and why it is named rather than guessed
 *
 * A settled row carrying no price cannot be added. Guessing zero is what the
 * tile formatter already refuses elsewhere — an absent field renders `—`, never
 * `$0.00` — so the result reports `exact: false` and lets the caller ask the
 * server. An UNSETTLED row without a price is ordinary, not missing: it has not
 * been charged yet.
 */
export type TileCounters = {
  spentNanos: number | undefined;
  eventsProcessed: number | undefined;
  /** False when a settled row carried no price, so the spend shown is a floor. */
  exact: boolean;
};

// A row the server has finished with, and therefore one that should carry a
// price. `RECEIVED` and `GATE_BLOCKED` are still in flight; `OPTIMISTIC` is the
// browser's own placeholder with no server row behind it yet.
const SETTLED: ReadonlySet<string> = new Set([
  AGENTSFLEET_EVENT_STATUS.PROCESSED,
  AGENTSFLEET_EVENT_STATUS.AGENT_ERROR,
  AGENTSFLEET_EVENT_STATUS.FAILED,
]);

type Base = { budget_used_nanos?: number; events_processed?: number };

/** One row's contribution: what it adds, and whether it could be priced. */
function contribution(event: FleetEvent): { nanos: number; counted: number; priced: boolean } {
  if (event.status === AGENTSFLEET_EVENT_STATUS.OPTIMISTIC) {
    return { nanos: 0, counted: 0, priced: true };
  }
  if (typeof event.costNanos === "number") {
    return { nanos: event.costNanos, counted: 1, priced: true };
  }
  return { nanos: 0, counted: 1, priced: !SETTLED.has(event.status) };
}

export function deriveTileCounters(base: Base, streamed: readonly FleetEvent[]): TileCounters {
  const delta = streamed.reduce(
    (acc, event) => {
      const one = contribution(event);
      return {
        nanos: acc.nanos + one.nanos,
        counted: acc.counted + one.counted,
        exact: acc.exact && one.priced,
      };
    },
    { nanos: 0, counted: 0, exact: true },
  );
  return {
    // An absent base stays absent: the tile renders `—` for a field the daemon
    // never sent, and a delta must not promote that into a number.
    spentNanos:
      base.budget_used_nanos === undefined ? undefined : base.budget_used_nanos + delta.nanos,
    eventsProcessed:
      base.events_processed === undefined ? undefined : base.events_processed + delta.counted,
    exact: delta.exact,
  };
}
