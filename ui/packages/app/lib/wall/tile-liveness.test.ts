import { describe, expect, it } from "vitest";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets-types";
import { CONNECTION_STATUS } from "@/tests/helpers/fleet-streams";
import {
  deriveTileLiveness,
  fleetRowState,
  formatTileEvents,
  formatTileSpend,
  SNAPSHOT_CAPPED_OR_ERRORED,
  tileShouldStream,
} from "./tile-liveness";

describe("deriveTileLiveness — the tile is always exactly one kind (Inv. 1)", () => {
  it("an archived fleet is drained regardless of connection", () => {
    // Even if a connection were somehow live, a drained fleet stays drained.
    expect(deriveTileLiveness(AGENTSFLEET_STATUS.ARCHIVED, CONNECTION_STATUS.LIVE).kind).toBe("drained");
    expect(tileShouldStream(AGENTSFLEET_STATUS.ARCHIVED)).toBe(false);
  });

  it("an active fleet with a live or connecting stream is live (2.1)", () => {
    expect(deriveTileLiveness(AGENTSFLEET_STATUS.ACTIVE, CONNECTION_STATUS.LIVE).kind).toBe("live");
    expect(deriveTileLiveness(AGENTSFLEET_STATUS.ACTIVE, CONNECTION_STATUS.CONNECTING).kind).toBe("live");
    expect(tileShouldStream(AGENTSFLEET_STATUS.ACTIVE)).toBe(true);
  });

  it("an active fleet whose stream is reconnecting degrades to snapshot, never blank (2.2, 2.3)", () => {
    const liveness = deriveTileLiveness(AGENTSFLEET_STATUS.ACTIVE, CONNECTION_STATUS.RECONNECTING);
    expect(liveness.kind).toBe("snapshot");
    if (liveness.kind === "snapshot") {
      expect(liveness.reason).toBe(SNAPSHOT_CAPPED_OR_ERRORED);
    }
  });

  it("an active fleet whose stream lost access degrades to snapshot, never a stale live", () => {
    expect(deriveTileLiveness(AGENTSFLEET_STATUS.ACTIVE, CONNECTION_STATUS.REVOKED)).toEqual({
      kind: "snapshot",
      reason: SNAPSHOT_CAPPED_OR_ERRORED,
    });
  });

  it("an installing fleet streams (its state is transient, not drained)", () => {
    expect(tileShouldStream(AGENTSFLEET_STATUS.INSTALLING)).toBe(true);
    expect(deriveTileLiveness(AGENTSFLEET_STATUS.INSTALLING, CONNECTION_STATUS.LIVE).kind).toBe("live");
  });
});

describe("tile footer is server truth (Inv. 2)", () => {
  it("formats spend from budget_used_nanos, never token math", () => {
    expect(formatTileSpend(1_200_000_000n)).toBe("$1.20");
    expect(formatTileSpend(0n)).toBe("$0.00");
  });

  it("renders large exact counters and rounds nanos to cents", () => {
    expect(formatTileSpend(9007199254740993n)).toBe("$9007199.25");
    expect(formatTileEvents(9007199254740993n)).toBe("9007199254740993");
    expect(formatTileSpend(999999999n)).toBe("$1.00");
  });

  it("renders zero and positive event counts", () => {
    expect(formatTileEvents(0n)).toBe("0");
    expect(formatTileEvents(7n)).toBe("7");
  });
});

describe("fleetRowState — lifecycle state for the tile link (e2e data-state)", () => {
  it("maps each status to its row state", () => {
    expect(fleetRowState(AGENTSFLEET_STATUS.ACTIVE)).toBe("live");
    expect(fleetRowState(AGENTSFLEET_STATUS.INSTALLING)).toBe("installing");
    expect(fleetRowState(AGENTSFLEET_STATUS.ARCHIVED)).toBe("archived");
    // The retired lifecycle values are no longer statuses the wall knows.
    expect(fleetRowState("stopped")).toBe("parked");
    // An unrecognized status falls back to parked (the neutral, non-live default).
    expect(fleetRowState("who_knows")).toBe("parked");
  });
});
