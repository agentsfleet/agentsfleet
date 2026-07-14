import { describe, expect, it } from "vitest";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets";
import { countFleets } from "./fleet-rollup";

// The dashboard tiles counted active/paused/stopped — three of five statuses. An
// `installing` or `killed` fleet was in the workspace, in the list, and in NO
// tile. These pin the property the old code lacked: every fleet is accounted for.

function fleet(status: string) {
  return { status };
}

describe("countFleets", () => {
  it("accounts for every fleet — the buckets sum to the total", () => {
    const fleets = [
      fleet(AGENTSFLEET_STATUS.ACTIVE),
      fleet(AGENTSFLEET_STATUS.ACTIVE),
      fleet(AGENTSFLEET_STATUS.PAUSED),
      fleet(AGENTSFLEET_STATUS.STOPPED),
      fleet(AGENTSFLEET_STATUS.INSTALLING),
      fleet(AGENTSFLEET_STATUS.KILLED),
    ];
    const counts = countFleets(fleets);
    const summed =
      Object.values(counts.byStatus).reduce((a, b) => a + b, 0) + counts.unknown;
    expect(summed).toBe(counts.total);
    expect(counts.total).toBe(fleets.length);
  });

  // The bug, stated directly: these two statuses used to land nowhere.
  it("counts installing and killed fleets rather than dropping them", () => {
    const counts = countFleets([
      fleet(AGENTSFLEET_STATUS.INSTALLING),
      fleet(AGENTSFLEET_STATUS.KILLED),
      fleet(AGENTSFLEET_STATUS.KILLED),
    ]);
    expect(counts.byStatus[AGENTSFLEET_STATUS.INSTALLING]).toBe(1);
    expect(counts.byStatus[AGENTSFLEET_STATUS.KILLED]).toBe(2);
    expect(counts.unknown).toBe(0);
  });

  it("gives every registered status a bucket, even at zero", () => {
    const counts = countFleets([]);
    for (const status of Object.values(AGENTSFLEET_STATUS)) {
      expect(counts.byStatus[status]).toBe(0);
    }
  });

  // A backend that ships a status ahead of the client must not silently vanish
  // from the rollup — it is counted as unknown, so the totals still reconcile.
  it("counts an unrecognised status rather than discarding it", () => {
    const counts = countFleets([fleet("hibernating"), fleet(AGENTSFLEET_STATUS.ACTIVE)]);
    expect(counts.unknown).toBe(1);
    expect(counts.total).toBe(2);
    const summed =
      Object.values(counts.byStatus).reduce((a, b) => a + b, 0) + counts.unknown;
    expect(summed).toBe(counts.total);
  });
});
