import { AGENTSFLEET_STATUS, type FleetStatus } from "@/lib/api/fleets";

// The dashboard's fleet rollup, derived in one place and TOTAL over the status
// registry.
//
// The tiles used to count `active`, `paused`, and `stopped` — three of the five
// statuses. A fleet that was `installing` or `killed` appeared in no tile at all:
// it was in the workspace, it was in the list, and the dashboard's own summary
// silently did not acknowledge it. The counts did not add up to the fleets, and
// nothing said so.
//
// Seeding the buckets from AGENTSFLEET_STATUS itself is what makes that
// unrepeatable. A status added to the registry gets a bucket for free, and
// `FleetRollup` being a Record over FleetStatus means a caller that forgets to
// render one fails the type check rather than quietly dropping it.

export type FleetRollup = Record<FleetStatus, number>;

/** Every fleet lands in exactly one bucket, and `unknown` is not a bucket — an
 *  unrecognised status is counted separately rather than discarded, so the
 *  totals can be asserted against the fleet count. */
export type FleetCounts = {
  byStatus: FleetRollup;
  /** A status the client does not know about — a backend that shipped ahead of us. */
  unknown: number;
  total: number;
};

function emptyRollup(): FleetRollup {
  const zeroed = {} as FleetRollup;
  for (const status of Object.values(AGENTSFLEET_STATUS)) zeroed[status] = 0;
  return zeroed;
}

export function countFleets(fleets: readonly { status: string }[]): FleetCounts {
  const byStatus = emptyRollup();
  let unknown = 0;
  for (const fleet of fleets) {
    if (fleet.status in byStatus) byStatus[fleet.status as FleetStatus] += 1;
    else unknown += 1;
  }
  return { byStatus, unknown, total: fleets.length };
}
