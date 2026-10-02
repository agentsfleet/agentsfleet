import { describe, expect, it, vi } from "vitest";
import { FRAME_KIND } from "@/lib/api/events-types";
import { ACTOR } from "@/lib/events/event-summary";
import { getSnapshot, subscribe } from "./fleet-stream-registry";
import { AGENTSFLEET_EVENT_STATUS } from "./fleet-stream-row";
import { row, setupRegistryTests, sourceAt, WS, Z_A } from "@/tests/helpers/fleet-stream-registry-fixtures";
import {
  RECONNECT_ADVANCE_MS,
  fetchSpy,
  flushBackfill,
  pageWith,
  queryOf,
  setupBackfillTests,
} from "@/tests/helpers/fleet-stream-backfill-fixtures";

setupRegistryTests();
setupBackfillTests();

const STEER_ID = "evt_steer";
// The steer's admission instant: its history row carries this, not its lease.
const ADMITTED_AT_MS = Date.UTC(2026, 4, 15, 18, 30, 0);
// A durable row a minute later, which is where the watermark stands.
const SEEDED_AT_MS = ADMITTED_AT_MS + 60_000;
const SEED_ID = "evt_seed";

// Opens the stream, lets a teammate's steer wait on it, then drops and
// restores the connection: the lease and its frames fall in the gap.
function waitThenReconnect(): void {
  const first = sourceAt(0);
  first.open();
  first.heartbeat();
  first.emit({
    kind: FRAME_KIND.EVENT_ADMITTED,
    event_id: STEER_ID,
    actor: `${ACTOR.STEER_PREFIX}user_bob`,
    message: "check the tests",
    created_at: ADMITTED_AT_MS,
  });
  first.fail();
  vi.advanceTimersByTime(RECONNECT_ADVANCE_MS);
  const second = sourceAt(1);
  second.open();
  second.heartbeat();
}

function steerStatus(): string | undefined {
  return getSnapshot(Z_A).events.find((event) => event.id === STEER_ID)?.status;
}

describe("fleet-stream-registry — a steer leased during an outage", () => {
  it("should reach back to the waiting row's admission and settle it from the recovered row", async () => {
    fetchSpy.mockResolvedValueOnce(pageWith([
      row({ event_id: SEED_ID, created_at: SEEDED_AT_MS }),
      row({ event_id: STEER_ID, created_at: ADMITTED_AT_MS, status: AGENTSFLEET_EVENT_STATUS.PROCESSED }),
    ]));
    const release = subscribe(WS, Z_A, [row({ event_id: SEED_ID, created_at: SEEDED_AT_MS })], () => {});

    waitThenReconnect();
    expect(steerStatus()).toBe(AGENTSFLEET_EVENT_STATUS.QUEUED);
    await flushBackfill();

    const since = queryOf(0).get("since");
    expect(since).not.toBeNull();
    expect(Date.parse(since ?? "")).toBeLessThanOrEqual(ADMITTED_AT_MS);
    expect(steerStatus()).toBe(AGENTSFLEET_EVENT_STATUS.PROCESSED);
    release();
  });

  it("should start the waiting row when the recovered row says it is still running", async () => {
    fetchSpy.mockResolvedValueOnce(pageWith([
      row({ event_id: STEER_ID, created_at: ADMITTED_AT_MS, status: AGENTSFLEET_EVENT_STATUS.RECEIVED }),
    ]));
    const release = subscribe(WS, Z_A, [row({ event_id: SEED_ID, created_at: SEEDED_AT_MS })], () => {});

    waitThenReconnect();
    await flushBackfill();

    expect(steerStatus()).toBe(AGENTSFLEET_EVENT_STATUS.RECEIVED);
    release();
  });

  it("should never move the watermark back to the waiting row's admission", async () => {
    fetchSpy
      .mockResolvedValueOnce(pageWith([
        row({ event_id: STEER_ID, created_at: ADMITTED_AT_MS, status: AGENTSFLEET_EVENT_STATUS.PROCESSED }),
      ]))
      .mockResolvedValueOnce(pageWith([]));
    const release = subscribe(WS, Z_A, [row({ event_id: SEED_ID, created_at: SEEDED_AT_MS })], () => {});

    waitThenReconnect();
    await flushBackfill();
    // Nothing waits now, so the next recovery starts from the seed again.
    sourceAt(1).fail();
    vi.advanceTimersToNextTimer();
    sourceAt(-1).open();
    sourceAt(-1).heartbeat();
    await flushBackfill();

    expect(fetchSpy).toHaveBeenCalledTimes(2);
    expect(Date.parse(queryOf(1).get("since") ?? "")).toBeGreaterThan(ADMITTED_AT_MS);
    release();
  });
});
