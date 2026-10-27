// @vitest-environment node
import { afterEach, beforeEach, expect, it } from "vitest";
import { fleetActivityText, FLEET_ACTIVITY } from "@/lib/events/fleet-activity";
import { draftThreadMessages } from "@/lib/api/draft-thread-messages";
import { EMPTY_DRAFT_VIEW } from "@/lib/api/draft-state";
import { DraftClientFixture } from "./helpers/draft-client-fixture";
import { DRAFT, DRAFT_ID, FLEET_DETAIL, FLEET_ID, MESSAGE, response } from "./helpers/draft-fixtures";

const fixture = new DraftClientFixture();
const RECEIPT = {event_id: "setup_saved:one", event_type: FLEET_ACTIVITY.type, status: FLEET_ACTIVITY.status,
  request_json: JSON.stringify({kind: "setup_saved", revision: "1"}), created_at: "2026-10-26T08:00:00Z"};
beforeEach(() => {fixture.mount(); fixture.fleet = FLEET_DETAIL;});
afterEach(() => fixture.dispose());

it("reads a durable setup occurrence again after the client is recreated", async () => {
  fixture.current = {...DRAFT, state: "closed", linked_fleet_id: FLEET_ID};
  fixture.override = ({path}) => path.includes("/fleets/") && path.endsWith("/messages")
    ? response({items: [RECEIPT], next_cursor: null}) : undefined;
  const first = await fixture.ready(true);
  expect(first.getSnapshot().activities).toEqual([RECEIPT]);
  first.dispose();
  const second = await fixture.ready(true);
  expect(second.getSnapshot().activities).toEqual([RECEIPT]);
  expect(fixture.mutations()).toEqual([]);
});

it("keeps setup saved before the next request and its working reply", () => {
  const later = {...MESSAGE, sequence: "2", state: "running", created_at: "2026-10-26T08:01:00Z"} as const;
  const projected = draftThreadMessages({...EMPTY_DRAFT_VIEW, messages: [later], activities: [RECEIPT],
    previousDrafts: [{id: DRAFT_ID, cancelledThrough: "0", messages: [MESSAGE]}]});
  expect(projected.map(row => row.id)).toEqual([`${DRAFT_ID}:1:user`, `activity:${RECEIPT.event_id}`, "2:user", "2:assistant"]);
});

it("refreshes access saved from history and readiness from the current database read", async () => {
  fixture.current = {...DRAFT, state: "closed", linked_fleet_id: FLEET_ID};
  const client = await fixture.ready(true);
  const access = {...RECEIPT, event_id: "access:fly", request_json: JSON.stringify({kind: "access_saved", service: "fly"})};
  fixture.override = ({path}) => path.includes("/fleets/") && path.endsWith("/messages")
    ? response({items: [access, RECEIPT], next_cursor: null}) : undefined;
  fixture.fleet = {...FLEET_DETAIL, setup: {state: "ready", checks: []}};
  await client.refreshFleetSetup(true);
  expect(client.getSnapshot()).toMatchObject({activities: [access, RECEIPT], fleet: {setup: {checks: []}}});
  expect(fleetActivityText(access.request_json)).toBe("Fly access saved");
  expect(fixture.mutations()).toEqual([]);
});

it.each([403, 503])("does not invent an access confirmation when history refuses %i", async status => {
  fixture.current = {...DRAFT, state: "closed", linked_fleet_id: FLEET_ID};
  const client = await fixture.ready(true);
  fixture.override = ({path}) => path.includes("/fleets/") && path.endsWith("/messages") ? response({}, status) : undefined;
  await expect(client.refreshFleetSetup(true)).rejects.toMatchObject({status});
  expect(client.getSnapshot().activities ?? []).toEqual([]);
  expect(fixture.mutations()).toEqual([]);
});

it("rejects a malformed history response rather than displaying unvalidated fields", async () => {
  fixture.current = {...DRAFT, state: "closed", linked_fleet_id: FLEET_ID};
  fixture.override = ({path}) => path.includes("/fleets/") && path.endsWith("/messages")
    ? response({items: [{...RECEIPT, created_at: "not a date"}], next_cursor: null}) : undefined;
  const client = await fixture.ready();
  await expect(client.resume(DRAFT_ID)).rejects.toMatchObject({status: 502});
  expect(client.getSnapshot().activities ?? []).toEqual([]);
});

it.each(["{", JSON.stringify({kind: "future_event", body: "must stay hidden"}),
  JSON.stringify({kind: "setup_saved", revision: 1}), JSON.stringify({kind: "access_saved"})])(
  "renders unknown or malformed activity neutrally without exposing its body (%s)", json => {
    expect(fleetActivityText(json)).toBe("Activity recorded");
  });

it("does not expose extra fields carried by an otherwise valid access event", () => {
  expect(fleetActivityText(JSON.stringify({kind: "access_saved", service: "fly", api_token: "unapproved-payload"}))).toBe("Fly access saved");
});

// Value: protects=safe readable persisted activity summaries; fails_when=permission states reverse or an extra payload field leaks into chat;
// why_new=existing valid-event assertions covered access only; seam=none; no_claim=database receipt storage.
it("renders saved readiness and each memory-sharing state from permitted activity fields only", () => {
  const hidden = "unapproved event payload";
  expect(fleetActivityText(JSON.stringify({kind: "setup_ready", revision: "3", body: hidden}))).toBe("Setup done · v3");
  for (const [read, publish, expected] of [
    [false, false, "Workspace memories: learning off, sharing off"],
    [false, true, "Workspace memories: learning off, sharing on"],
    [true, false, "Workspace memories: learning on, sharing off"],
    [true, true, "Workspace memories: learning on, sharing on"],
  ]) {
    expect(fleetActivityText(JSON.stringify({kind: "memory_sharing_changed", read, publish, body: hidden}))).toBe(expected);
  }
  expect(fleetActivityText(JSON.stringify({kind: "setup_ready", revision: "not a revision", body: hidden}))).toBe("Activity recorded");
});
