// @vitest-environment node
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { DRAFT_EVENT } from "@/lib/api/draft-events";
import { MESSAGE_STATE } from "@/lib/api/draft-types";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets-types";
import { DraftClientFixture } from "./helpers/draft-client-fixture";
import { FakeEventSource } from "./helpers/fake-event-source";
import { DRAFT, DRAFT_ID, FLEET_DETAIL, FLEET_ID, MESSAGE, REVIEW, response } from "./helpers/draft-fixtures";

const fixture = new DraftClientFixture();
const fleetPath = `/fleets/${FLEET_ID}`;
beforeEach(() => {
  fixture.mount(); fixture.fleet = FLEET_DETAIL;
  fixture.current = {...DRAFT, linked_fleet_id: FLEET_ID,
    review: {...REVIEW, effective: {...REVIEW.effective, target_setup_revision: "1"}}};
});
afterEach(() => fixture.dispose());

// Value: protects=stopped fleets stay stopped during chat recovery; fails_when=Retry treats a read failure as Start;
// why_new=covers a saved edit and a concurrent stop; seam=HTTP only; no_claim=database lifecycle enforcement.
it.each([AGENTSFLEET_STATUS.STOPPED, AGENTSFLEET_STATUS.INSTALLING])("preserves a fresh stop when retrying a saved %s edit", async status => {
  let refuseRead = false;
  fixture.fleet = {...FLEET_DETAIL, status};
  fixture.override = ({path, method}) => {
    if (path.endsWith("/revisions")) {
      fixture.fleet = {...fixture.fleet, setup_revision: "2"};
      return response({fleet_id: FLEET_ID, setup_revision: "2", setup_etag: '"two"', etag: '"two"', schedules: []}, 201);
    }
    if (path.endsWith(fleetPath) && method === "GET" && refuseRead) return response({user_message: "Read unavailable"}, 503);
    return undefined;
  };
  const client = await fixture.ready(true);
  await client.apply();
  refuseRead = true;
  await expect(client.refreshSetup()).rejects.toMatchObject({status: 503});
  refuseRead = false;
  fixture.fleet = {...fixture.fleet, status: AGENTSFLEET_STATUS.STOPPED};
  await client.retry();
  expect(client.getSnapshot()).toMatchObject({fleet: {status: AGENTSFLEET_STATUS.STOPPED}, draft: {id: DRAFT_ID}, messages: [MESSAGE], busy: false});
  expect(fixture.mutations().map(call => ({method: call.method, path: call.path.split("/").at(-1)})))
    .toEqual([{method: "POST", path: "revisions"}]);
});

// Value: protects=Finish setup after Apply and access storage; fails_when=a retained chat prevents activating its saved revision;
// why_new=reopening tests do not Apply in the same session; seam=HTTP only; no_claim=database activation or credential storage.
it("finishes an applied setup after access is refreshed without losing its conversation", async () => {
  fixture.override = ({path, method}) => {
    if (path.endsWith("/revisions")) {
      fixture.fleet = {...FLEET_DETAIL, setup_revision: "2"};
      return response({fleet_id: FLEET_ID, setup_revision: "2", setup_etag: '"two"', etag: '"two"', schedules: []}, 201);
    }
    if (path.endsWith(fleetPath) && method === "PATCH") {
      fixture.current = {...fixture.current, state: "closed"};
      fixture.fleet = {...fixture.fleet, status: "active", setup: {state: "ready", checks: []}};
      return response(fixture.fleet);
    }
    return undefined;
  };
  const client = await fixture.ready(true);
  await client.apply();
  expect(client.getSnapshot()).toMatchObject({savedSetupRevision: "2", draft: {id: DRAFT_ID}, fleet: {status: "installing"}});
  fixture.fleet = {...fixture.fleet, setup: {state: "ready", checks: []}};
  await client.refreshFleetSetup(true);
  await client.retry();
  expect(client.getSnapshot()).toMatchObject({savedSetupRevision: "2", draft: {id: DRAFT_ID, state: "closed"},
    fleet: {status: "active", setup_revision: "2"}, messages: [MESSAGE], busy: false, pending: false});
  expect(fixture.mutations().map(call => ({method: call.method, path: call.path.split("/").at(-1)})))
    .toEqual([{method: "POST", path: "revisions"}, {method: "PATCH", path: FLEET_ID}]);
  expect(fixture.mutations()[1]?.body).toEqual({status: "active"});
  expect(fixture.mutations()[1]?.headers.get("If-Match")).toBe(fixture.fleet.etag);
});

// Value: protects=access changes refresh the proposal the person sees; fails_when=Check again leaves a stale credential warning;
// why_new=saved fleet readiness differs from the editable proposal; seam=HTTP only; no_claim=provider authorization.
it("refreshes the editable review after access changes without saving the proposal", async () => {
  fixture.current = {...fixture.current, review: {...fixture.current.review!, readiness: FLEET_DETAIL.setup.checks}};
  const client = await fixture.ready(true);
  await client.refreshSetup();
  expect(client.getSnapshot().fleet?.id).toBe(FLEET_ID);
  expect(client.getSnapshot().draft?.review?.readiness).toHaveLength(1);
  fixture.current = {...fixture.current, review: {...fixture.current.review!, readiness: []}};
  await client.refreshFleetSetup(true);
  expect(client.getSnapshot()).toMatchObject({draft: {id: DRAFT_ID, review: {readiness: []}}, savedSetupRevision: null, messages: [MESSAGE]});
  expect(fixture.mutations()).toEqual([]);
});

// Value: protects=running edits complete after reopening; fails_when=the restored turn has no live stream;
// why_new=resume starts a watcher but the fleet edit entrypoint differs; seam=browser EventSource and HTTP only; no_claim=live transport.
it("follows a running edit when opening the saved fleet's current conversation", async () => {
  let running = true;
  fixture.override = ({path, method}) => path.endsWith("/authoring-context")
    ? response({fleet: fixture.fleet, conversations: [{draft: fixture.current,
      messages: [{...MESSAGE, state: MESSAGE_STATE.running}], messages_starting_after: null, truncated: false}],
      activities: {items: [], next_cursor: null}, next_cursor: null})
    : path.includes("/drafts/") && path.endsWith("/messages") && method === "GET"
      ? response({items: [{...MESSAGE, state: running ? MESSAGE_STATE.running : MESSAGE_STATE.completed}], next_cursor: null}) : undefined;
  const client = await fixture.ready();
  client.fresh({kind: "fleet", fleet_id: FLEET_ID, setup_revision: "1"});
  await client.refreshFleetSetup();
  expect(client.getSnapshot().messages[0]?.state).toBe(MESSAGE_STATE.running);
  expect(FakeEventSource.instances).toHaveLength(1);
  const stream = FakeEventSource.instances[0]!;
  stream.open();
  await vi.waitFor(() => expect(fixture.calls.filter(call => call.path.includes("/drafts/") && call.path.endsWith("/messages"))).toHaveLength(1));
  running = false;
  stream.emitNamed(DRAFT_EVENT.completed, JSON.stringify({draft_id: DRAFT_ID}));
  await vi.waitFor(() => expect(client.getSnapshot().messages[0]?.state).toBe(MESSAGE_STATE.completed));
  expect(stream.closed).toBe(true);
  expect(fixture.mutations()).toEqual([]);
});
