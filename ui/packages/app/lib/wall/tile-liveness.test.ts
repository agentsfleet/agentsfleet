import { describe, expect, it } from "vitest";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets";
import { CONNECTION_STATUS } from "@/lib/streaming/fleet-stream-registry";
import {
  deriveTileLiveness,
  fleetRowState,
  formatTileEvents,
  formatTileSpend,
  SNAPSHOT_CAPPED_OR_ERRORED,
  tileShouldStream,
} from "./tile-liveness";

describe("deriveTileLiveness — the tile is always exactly one kind (Inv. 1)", () => {
  it("parked, paused, and killed fleets are drained regardless of connection", () => {
    for (const status of [
      AGENTSFLEET_STATUS.STOPPED,
      AGENTSFLEET_STATUS.PAUSED,
      AGENTSFLEET_STATUS.KILLED,
    ]) {
      // Even if a connection were somehow live, a drained fleet stays drained.
      expect(deriveTileLiveness(status, CONNECTION_STATUS.LIVE).kind).toBe("drained");
      expect(tileShouldStream(status)).toBe(false);
    }
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

  it("an installing fleet streams (its state is transient, not drained)", () => {
    expect(tileShouldStream(AGENTSFLEET_STATUS.INSTALLING)).toBe(true);
    expect(deriveTileLiveness(AGENTSFLEET_STATUS.INSTALLING, CONNECTION_STATUS.LIVE).kind).toBe("live");
  });
});

describe("tile footer is server truth (Inv. 2)", () => {
  it("formats spend from budget_used_nanos, never token math", () => {
    expect(formatTileSpend(1_200_000_000)).toBe("$1.20");
    expect(formatTileSpend(0)).toBe("$0.00");
  });

  it("renders a dash — not $0.00 — when the daemon did not send the field", () => {
    expect(formatTileSpend(undefined)).toBe("—");
    expect(formatTileEvents(undefined)).toBe("—");
  });

  it("renders a real zero distinctly from a missing field", () => {
    expect(formatTileEvents(0)).toBe("0");
    expect(formatTileEvents(7)).toBe("7");
  });
});

describe("fleetRowState — lifecycle state for the tile link (e2e data-state)", () => {
  it("maps each status to its row state", () => {
    expect(fleetRowState(AGENTSFLEET_STATUS.ACTIVE)).toBe("live");
    expect(fleetRowState(AGENTSFLEET_STATUS.INSTALLING)).toBe("installing");
    expect(fleetRowState(AGENTSFLEET_STATUS.KILLED)).toBe("failed");
    expect(fleetRowState(AGENTSFLEET_STATUS.STOPPED)).toBe("parked");
    expect(fleetRowState(AGENTSFLEET_STATUS.PAUSED)).toBe("parked");
    // An unrecognized status falls back to parked (the neutral, non-live default).
    expect(fleetRowState("who_knows")).toBe("parked");
  });
});
