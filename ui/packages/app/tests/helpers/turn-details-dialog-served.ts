import type { TurnDetail } from "@/lib/api/turns";

// The dialog reads bodies through the Server Action (the list row carries
// none). Every fixture is already detail-shaped, so the action serves back the
// row the test opened. This module imports nothing under test: each shard's
// hoisted `vi.mock` factory loads it, and a factory that reached the dialog
// would wait on the module graph it is being loaded for.
let servedDetail: TurnDetail | null = null;

export function serveDetail(row: TurnDetail): void {
  servedDetail = row;
}

export function fleetActionsMock() {
  return {
    getFleetTurnAction: () =>
      Promise.resolve(
        servedDetail === null
          ? { ok: false as const, error: "not found" }
          : { ok: true as const, data: servedDetail },
      ),
  };
}
