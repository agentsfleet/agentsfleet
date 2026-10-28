// @vitest-environment node
import { afterEach, beforeEach, expect, it } from "vitest";
import { updateSavedSetup } from "@/lib/api/draft-activity";
import { SetupUnavailable } from "@/lib/api/draft-errors";
import { EMPTY_DRAFT_VIEW } from "@/lib/api/draft-state";
import type { DraftView } from "@/lib/api/draft-state";
import { DRAFT_STATE } from "@/lib/api/draft-types";
import { DraftClientFixture } from "./helpers/draft-client-fixture";
import { DRAFT, DRAFT_ID, FLEET_DETAIL, FLEET_ID, MESSAGE, REVIEW, response } from "./helpers/draft-fixtures";

const fixture = new DraftClientFixture();
const historyFailure = {user_message: "History temporarily unavailable"};
beforeEach(() => fixture.mount());
afterEach(() => fixture.dispose());

// Value: protects=confirmed writes settle despite optional history outages; fails_when=the receipt read leaves a write pending;
// why_new=write replay tests fail the required fleet read instead; seam=HTTP; no_claim=database atomicity.
it("settles an applied revision when its following receipt read fails and permits another edit", async () => {
  fixture.fleet = FLEET_DETAIL;
  fixture.current = {...DRAFT, linked_fleet_id: FLEET_ID,
    review: {...REVIEW, effective: {...REVIEW.effective, target_setup_revision: "1"}}};
  fixture.override = ({path, method}) => {
    if (path.endsWith("/revisions")) {
      fixture.fleet = {...FLEET_DETAIL, setup_revision: "2"};
      return response({fleet_id: FLEET_ID, setup_revision: "2", setup_etag: '"two"', etag: '"two"', schedules: []}, 201);
    }
    if (path.includes("/fleets/") && path.endsWith("/messages")) return response(historyFailure, 502);
    if (method === "PATCH" && path.endsWith(`/drafts/${DRAFT_ID}`)) return response(fixture.current);
    return undefined;
  };
  const client = await fixture.ready(true);
  await client.apply();
  expect(client.getSnapshot()).toMatchObject({savedSetupRevision: "2", fleet: {setup_revision: "2"}, pending: false, error: null});
  await client.rename("Updated conversation");
  expect(client.getSnapshot().pending).toBe(false);
  expect(fixture.calls.filter(call => call.path.endsWith("/revisions"))).toHaveLength(1);
  expect(fixture.mutations().at(-1)?.body).toEqual({title: "Updated conversation"});
});

// Value: protects=creator conversation restoration during a receipt outage; fails_when=optional history blocks resume;
// why_new=opening editable drafts does not load saved fleet receipts; seam=HTTP; no_claim=creator authorization in storage.
it("restores a saved draft conversation when receipt history returns 502", async () => {
  fixture.current = {...DRAFT, state: DRAFT_STATE.closed, linked_fleet_id: FLEET_ID};
  fixture.override = ({path}) => path.includes("/fleets/") && path.endsWith("/messages") ? response(historyFailure, 502) : undefined;
  const client = await fixture.ready(true);
  expect(client.getSnapshot()).toMatchObject({draft: {id: DRAFT_ID}, messages: [MESSAGE], fleet: {id: FLEET_ID}, pending: false, error: null});
});

// Value: protects=previous receipts remain visible across optional read failure; fails_when=transient failure erases confirmed history;
// why_new=client fixtures start with empty activity; seam=HTTP and view patch; no_claim=receipt persistence.
it("retains prior receipts while recording the confirmed saved revision", async () => {
  const activities = [{event_id: "saved-one", event_type: "activity", status: "recorded", request_json: "{}", created_at: MESSAGE.created_at}];
  let view: DraftView = {...EMPTY_DRAFT_VIEW, activities};
  fixture.override = () => response(historyFailure, 502);
  await updateSavedSetup("/setup", FLEET_DETAIL, new AbortController().signal, patch => {view = {...view, ...patch};}, {});
  expect(view).toMatchObject({fleet: FLEET_DETAIL, savedSetupRevision: "1"});
  expect(view.activities).toBe(activities);
});

// Value: protects=receipt access revocation reaches the clearing owner; fails_when=best-effort handling swallows an authorization loss;
// why_new=transient outages must differ from lost access; seam=HTTP; no_claim=authentication middleware.
it.each([401, 403, 404, 410])("propagates receipt access loss with status %s", async status => {
  fixture.override = () => response(historyFailure, status);
  await expect(updateSavedSetup("/setup", FLEET_DETAIL, new AbortController().signal, () => undefined, {}))
    .rejects.toMatchObject({status});
});

// Value: protects=cancelled views never consume a later optional receipt response; fails_when=the owner abort is swallowed;
// why_new=timeout and owner cancellation have different ownership; seam=HTTP; no_claim=server cancellation.
it("propagates cancellation rather than reporting a completed optional read", async () => {
  const owner = new AbortController();
  fixture.override = () => {owner.abort(); throw new Error("read cancelled");};
  await expect(updateSavedSetup("/setup", FLEET_DETAIL, owner.signal, () => undefined, {})).rejects.toBeInstanceOf(SetupUnavailable);
});
