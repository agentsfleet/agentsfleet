import type { EventDetail } from "@/lib/api/events";

// The dialog reads bodies through the Server Action (the list row carries
// none). Every fixture is already detail-shaped, so the action serves back the
// row the test opened. This module imports nothing under test: each shard's
// hoisted `vi.mock` factory loads it, and a factory that reached the dialog
// would wait on the module graph it is being loaded for.
let servedDetail: EventDetail | null = null;

export function serveDetail(row: EventDetail): void {
  servedDetail = row;
}

export function fleetActionsMock() {
  return {
    getFleetEventAction: () =>
      Promise.resolve(
        servedDetail === null
          ? { ok: false as const, error: "not found" }
          : { ok: true as const, data: servedDetail },
      ),
  };
}
