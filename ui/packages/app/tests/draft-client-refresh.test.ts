// @vitest-environment node
import { afterEach, beforeEach, expect, it } from "vitest";
import { SOURCE_KIND, SCHEDULE_SYNC } from "@/lib/api/draft-vocabulary";
import { DraftClientFixture } from "./helpers/draft-client-fixture";
import { DRAFT_ID, FLEET_ID, REVIEW, WORKSPACE, response } from "./helpers/draft-fixtures";

const fixture = new DraftClientFixture();
const source = {kind: SOURCE_KIND.definition, definition: REVIEW.effective.definition};
beforeEach(() => fixture.mount());
afterEach(() => fixture.dispose());

it("previews an explicit source before a draft exists without creating a fleet", async () => {
  const client = await fixture.ready();
  expect(await client.refreshSetup(source)).toMatchObject({definition: source.definition});
  expect(client.getSnapshot()).toMatchObject({draft: null, fleet: null, preview: {definition: source.definition}});
  expect(fixture.mutations()).toEqual([]);
});

it("preserves the current conversation when a different refresh source is supplied", async () => {
  const client = await fixture.ready(true);
  expect(await client.refreshSetup(source)).toMatchObject({definition: REVIEW.effective.definition});
  expect(client.getSnapshot()).toMatchObject({draft: {id: DRAFT_ID}, preview: null, fleet: null});
  expect(fixture.mutations()).toEqual([]);
});

it("refreshes readiness from the source preview before any fleet has been saved", async () => {
  const client = await fixture.ready();
  client.fresh(source);
  await client.refreshFleetSetup(true);
  expect(client.getSnapshot()).toMatchObject({draft: null, fleet: null, preview: {definition: source.definition}, busy: false});
  expect(fixture.mutations()).toEqual([]);
});

it("refuses a fleet history read for a definition source without a network request", async () => {
  const client = await fixture.ready();
  client.fresh(source);
  const before = fixture.calls.length;
  await expect(client.refreshFleetSetup()).rejects.toMatchObject({status: 409, message: "Choose a fleet before opening its setup."});
  expect(fixture.calls).toHaveLength(before);
  expect(client.getSnapshot()).toMatchObject({draft: null, fleet: null, busy: false, pending: false});
});

it("refuses a readiness refresh without instructions and leaves setup unsaved", async () => {
  const client = await fixture.ready();
  await expect(client.refreshFleetSetup(true)).rejects.toMatchObject({status: 409, message: "There is no setup to refresh yet."});
  expect(client.getSnapshot()).toMatchObject({draft: null, fleet: null, preview: null, busy: false});
  expect(fixture.mutations()).toEqual([]);
});

it("retries a refused schedule registration before the saved fleet is loaded", async () => {
  const client = await fixture.ready();
  const schedule = {schedule_id: "pending-registration", fleet_id: FLEET_ID, sync: SCHEDULE_SYNC.failed};
  const registered = {...schedule, sync: SCHEDULE_SYNC.synced};
  const refused = "Scheduler unavailable";
  let attempts = 0;
  fixture.override = () => ++attempts === 1 ? response({user_message: refused}, 503) : response(registered);
  await expect(client.syncSchedule(schedule)).rejects.toMatchObject({status: 503, message: refused});
  expect(client.getSnapshot()).toMatchObject({fleet: null, schedules: [], busy: false});
  await client.syncSchedule(schedule);
  expect(client.getSnapshot()).toMatchObject({fleet: null, schedules: [], busy: false});
  expect(fixture.mutations()).toHaveLength(2);
  const path = `/live/v1/workspaces/${WORKSPACE}/fleets/${FLEET_ID}/schedules/${schedule.schedule_id}/sync`;
  expect(fixture.mutations().map(call => call.path)).toEqual([path, path]);
});
