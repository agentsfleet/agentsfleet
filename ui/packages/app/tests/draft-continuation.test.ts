// @vitest-environment node
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { DraftView } from "@/lib/api/draft-state";
import { DRAFT_STATE } from "@/lib/api/draft-types";
import { continueSavedDraft, restoreFleetHistory } from "@/lib/api/draft-continuation";
import { DraftClientFixture } from "./helpers/draft-client-fixture";
import { DRAFT, DRAFT_ID, FLEET_DETAIL, FLEET_ID, MESSAGE, REVIEW, response } from "./helpers/draft-fixtures";

const fixture = new DraftClientFixture();
const NEXT_DRAFT = "0199a801-1000-7000-8000-00000000000a";
const SAVED = {...DRAFT, state: "closed", linked_fleet_id: FLEET_ID} as const;
const BASE = "/live/v1/workspaces/workspace";
const cards = (...drafts: (typeof SAVED)[]) => drafts.map(draft => ({...draft,
  updated_at: MESSAGE.created_at, candidate_revision: 1, setup_revision: 1, publication_revision: null}));
beforeEach(() => {fixture.mount(); fixture.fleet = FLEET_DETAIL;});
afterEach(() => fixture.dispose());

// Value: protects=access retries activate the saved fleet without creating a new setup; fails_when=reopened setup pages require a retained draft or omit the fresh tag;
// why_new=the retained-chat test never retried a fleet-only setup page; seam=none; no_claim=server activation (mocked HTTP).
it.each(["retained chat", "setup page"])("reloads a fleet from its %s and resumes it with its fresh resource tag", async origin => {
  fixture.current = {...DRAFT, state: DRAFT_STATE.saving, linked_fleet_id: FLEET_ID};
  const client = await fixture.ready(origin === "retained chat");
  if (origin === "setup page") {
    client.fresh({kind: "fleet", fleet_id: FLEET_ID, setup_revision: "1"});
    await client.refreshFleetSetup();
    expect(client.getSnapshot().draft).toBeNull();
  }
  expect(client.getSnapshot().fleet?.id).toBe(FLEET_ID); expect(fixture.mutations()).toEqual([]);
  fixture.override = ({path, method}) => path.endsWith(`/fleets/${FLEET_ID}`) && method === "PATCH"
    ? response({...FLEET_DETAIL, status: "active"}) : undefined;
  await client.retry();
  expect(fixture.mutations()).toHaveLength(1); expect(fixture.mutations()[0]?.method).toBe("PATCH");
  expect(fixture.mutations()[0]?.headers.get("If-Match")).toBe(FLEET_DETAIL.etag);
  expect(fixture.mutations()[0]?.body).toEqual({status: "active"});
  expect(client.getSnapshot()).toMatchObject({pending: false, savedSetupRevision: "1", fleet: {id: FLEET_ID, status: "active"}});
});

// Value: protects=newly saved fleets continue from their current revision before the chat read links them;
// fails_when=continuation requires a linked draft or preinitialized history; seam=none; no_claim=server persistence (mocked HTTP).
it.each(["reopened", "newly saved"])("continues a %s setup from its current revision and retains the earlier conversation", async origin => {
  fixture.current = origin === "reopened" ? SAVED : DRAFT;
  const client = await fixture.ready(origin === "reopened");
  if (origin === "newly saved") {
    await client.edit({definition: REVIEW.effective.definition, instanceName: REVIEW.effective.instance_name});
    await client.save();
    expect(client.getSnapshot()).toMatchObject({draft: {id: DRAFT_ID}, savedSetupRevision: "1", fleet: {id: FLEET_ID}});
    expect(client.getSnapshot().draft?.linked_fleet_id).toBeUndefined();
  }
  const writes = fixture.mutations().length;
  fixture.fleet = {...FLEET_DETAIL, setup_revision: "3"};
  fixture.override = ({path, method}) => {
    if (path.endsWith("/drafts") && method === "POST") {
      fixture.current = {...DRAFT, id: NEXT_DRAFT, linked_fleet_id: FLEET_ID};
      return response({...fixture.current, accepted: {status: "accepted", sequence: "1", replayed: false}}, 201);
    }
    if (path.endsWith(`/drafts/${NEXT_DRAFT}`)) return response(fixture.current);
    return undefined;
  };
  await client.send("Add Slack notifications");
  expect(fixture.mutations()[writes]?.body).toMatchObject({source: {kind: "fleet", fleet_id: FLEET_ID, setup_revision: "3"}});
  expect(client.getSnapshot().previousDrafts?.[0]).toMatchObject({id: DRAFT_ID, messages: [MESSAGE]});
  expect(client.getSnapshot().draft?.id).toBe(NEXT_DRAFT);
});

// Value: protects=an incomplete saved view cannot silently create an unrelated blank setup;
// fails_when=the missing fleet guard disappears; seam=none; no_claim=normal client ordering (this exported boundary accepts view-shaped consumers).
it("refuses to continue a saved view without a fleet identity before making any request", async () => {
  const client = await fixture.ready();
  const view: DraftView = {...client.getSnapshot(), draft: DRAFT, savedSetupRevision: "1"};
  const calls = fixture.calls.length;
  await expect(continueSavedDraft(BASE, () => view, new AbortController().signal)).rejects.toMatchObject({
    status: 409, message: "The saved fleet could not be identified.",
  });
  expect(fixture.calls).toHaveLength(calls);
  expect(view.draft).toBe(DRAFT);
});

it("opens Edit setup with the creator-scoped history read and no mutation", async () => {
  fixture.current = SAVED;
  fixture.override = ({path}) => path.endsWith("/drafts") ? response({items: [{...SAVED,
    updated_at: MESSAGE.created_at, candidate_revision: 1, setup_revision: 1, publication_revision: null}], next_cursor: null}) : undefined;
  const client = await fixture.ready();
  client.fresh({kind: "fleet", fleet_id: FLEET_ID, setup_revision: "1"});
  await client.refreshFleetSetup();
  expect(client.getSnapshot().previousDrafts?.[0]?.messages).toEqual([MESSAGE]);
  expect(fixture.calls.some(call => call.path.endsWith("/drafts") && new URLSearchParams(call.search).get("linked_fleet_id") === FLEET_ID)).toBe(true);
  expect(fixture.mutations()).toEqual([]);
});

it("restores earlier saved conversations after reopening without executing historical Save messages", async () => {
  const latest = {...SAVED, id: NEXT_DRAFT};
  fixture.override = ({path}) => {
    if (path.endsWith("/drafts")) return response({items: [latest, SAVED].map(draft => ({...draft,
      updated_at: MESSAGE.created_at, candidate_revision: 1, setup_revision: 1, publication_revision: null})), next_cursor: null});
    if (path.endsWith(`/drafts/${NEXT_DRAFT}`)) return response(latest);
    if (path.endsWith(`/drafts/${DRAFT_ID}`)) return response(SAVED);
    if (path.endsWith("/messages") && path.includes("/drafts/")) return response({items: [{...MESSAGE, text: "Save"}], next_cursor: null});
    return undefined;
  };
  const client = await fixture.ready();
  client.fresh({kind: "fleet", fleet_id: FLEET_ID, setup_revision: "1"});
  await client.refreshFleetSetup();
  await vi.waitFor(() => expect(client.getSnapshot().previousDrafts?.map(draft => draft.id)).toEqual([DRAFT_ID, NEXT_DRAFT]));
  expect(fixture.mutations()).toEqual([]);
  expect(client.getSnapshot().saveOperation).toBeNull();
});

// Value: protects=failed setup reads never open another fleet's private chat; fails_when=revision, definition or ownership validation is removed;
// why_new=the stale-link test did not cover incomplete details or a mismatched history response; seam=none; no_claim=server authorization (mocked HTTP).
it.each([
  {failure: "stale revision", status: 409, message: "This fleet changed. Open Edit setup from its current setup."},
  {failure: "missing instructions", status: 502, message: "The saved instructions could not be loaded."},
  {failure: "wrong conversation", status: 409, message: "The setup chat does not belong to this fleet."},
])("refuses Edit setup after $failure without changing a draft", async ({failure, status, message}) => {
  const client = await fixture.ready();
  if (failure === "stale revision") fixture.fleet = {...FLEET_DETAIL, setup_revision: "2"};
  if (failure === "missing instructions") fixture.fleet = {...FLEET_DETAIL, definition: undefined};
  if (failure === "wrong conversation") {
    fixture.current = {...SAVED, linked_fleet_id: NEXT_DRAFT};
    fixture.override = ({path}) => path.endsWith("/drafts") ? response({items: cards(SAVED), next_cursor: null}) : undefined;
  }
  client.fresh({kind: "fleet", fleet_id: FLEET_ID, setup_revision: "1"});
  await expect(client.refreshFleetSetup()).rejects.toMatchObject({name: "SetupUnavailable", status, message});
  expect(client.getSnapshot().preview).toBeNull();
  expect(client.getSnapshot().messages).toEqual([]);
  expect(client.getSnapshot().draft).toBeNull();
  expect(fixture.mutations()).toEqual([]);
});

// Value: protects=unreadable saved instructions never start a replacement edit; fails_when=an incomplete fleet is accepted as an authoring source;
// why_new=existing refusal cases only denied access; seam=none; no_claim=server fleet completeness (mocked HTTP).
it.each([403, 404, 502])("does not replace saved instructions or start another draft after a %i read refusal", async status => {
  fixture.current = SAVED;
  const client = await fixture.ready(true);
  const before = client.getSnapshot();
  fixture.override = ({path}) => path.endsWith(`/fleets/${FLEET_ID}`)
    ? status === 502 ? response({...FLEET_DETAIL, definition: undefined}) : response({}, status) : undefined;
  await expect(client.send("Change the setup")).rejects.toMatchObject({name: "SetupUnavailable", status,
    ...(status === 502 ? {message: "The saved instructions could not be loaded."} : {})});
  expect(fixture.mutations()).toEqual([]);
  if (status === 502) {
    expect(client.getSnapshot()).toMatchObject({draft: SAVED, inaccessible: false, messages: [MESSAGE]});
    expect(client.getSnapshot().fleet).toBe(before.fleet);
  }
  else expect(client.getSnapshot()).toMatchObject({draft: null, fleet: null, inaccessible: true});
});

it("does not silently archive an earlier setup request that is still running", async () => {
  fixture.current = SAVED;
  fixture.override = ({path}) => path.endsWith("/drafts") ? response({items: [{...SAVED,
    updated_at: MESSAGE.created_at, candidate_revision: 1, setup_revision: 1, publication_revision: null}], next_cursor: null})
    : path.includes("/drafts/") && path.endsWith("/messages") ? response({items: [{...MESSAGE, state: "running"}], next_cursor: null}) : undefined;
  const client = await fixture.ready();
  client.fresh({kind: "fleet", fleet_id: FLEET_ID, setup_revision: "1"});
  await expect(client.refreshFleetSetup()).rejects.toMatchObject({status: 409, message: "Finish or stop your earlier setup request before opening a new edit."});
  expect(fixture.mutations()).toEqual([]);
});

// Value: protects=accepted setup changes remain retryable until readiness reaches the committed revision; fails_when=stale readiness is accepted or retry changes identity;
// why_new=HTTP failure coverage did not reject a successful stale read; seam=none; no_claim=server idempotency (modeled at the HTTP boundary).
it.each(["unavailable", "stale"])("retries an accepted Apply with the same key after an %s readiness read", async failure => {
  fixture.current = {...DRAFT, linked_fleet_id: FLEET_ID,
    review: {...REVIEW, effective: {...REVIEW.effective, target_setup_revision: "1"}}};
  let applied = false;
  let loseRead = true;
  const effects = new Set<string | null>();
  fixture.override = ({path, method, headers}) => {
    if (path.endsWith("/revisions")) {
      effects.add(headers.get("Idempotency-Key"));
      applied = true;
      return response({fleet_id: FLEET_ID, setup_revision: "2", setup_etag: '"two"', etag: '"two"', schedules: []});
    }
    if (path.endsWith(`/fleets/${FLEET_ID}`) && method === "GET" && applied) {
      if (loseRead) {
        loseRead = false;
        return failure === "unavailable" ? response({}, 503) : response(FLEET_DETAIL);
      }
      return response({...FLEET_DETAIL, setup_revision: "2"});
    }
    return undefined;
  };
  const client = await fixture.ready(true);
  await expect(client.apply()).rejects.toMatchObject({name: "SetupUnavailable", status: failure === "unavailable" ? 503 : 502,
    ...(failure === "stale" ? {message: "Setup was saved, but its latest readiness could not be loaded. Retry to check it."} : {})});
  expect(client.getSnapshot()).toMatchObject({pending: true, savedSetupRevision: null, fleet: {setup_revision: "1"}});
  expect(effects.size).toBe(1);
  await client.retry();
  const writes = fixture.mutations();
  expect(writes).toHaveLength(2);
  expect(writes[0]?.headers.get("Idempotency-Key")).toBe(writes[1]?.headers.get("Idempotency-Key"));
  expect(writes[0]?.headers.get("Idempotency-Key")).toBeTruthy();
  expect(writes[1]?.body).toEqual(writes[0]?.body);
  expect(effects.size).toBe(1);
  expect(client.getSnapshot()).toMatchObject({pending: false, savedSetupRevision: "2", fleet: {setup_revision: "2"}});
});

// Value: protects=reopening a current creator-owned edit keeps its proposal and conversation; fails_when=matching edits are archived or replaced;
// why_new=existing reopen cases only loaded completed conversations; seam=none; no_claim=creator authorization (mocked HTTP).
it("resumes a creator's editable conversation at the fleet's current setup revision", async () => {
  fixture.current = {...DRAFT, linked_fleet_id: FLEET_ID,
    review: {...REVIEW, effective: {...REVIEW.effective, target_setup_revision: "1"}}};
  const client = await fixture.ready();
  fixture.override = ({path}) => path.endsWith("/drafts")
    ? response({items: [{...cards(SAVED)[0], state: DRAFT.state}], next_cursor: null}) : undefined;
  client.fresh({kind: "fleet", fleet_id: FLEET_ID, setup_revision: "1"});
  await client.refreshFleetSetup();
  expect(client.getSnapshot()).toMatchObject({draft: fixture.current, messages: [MESSAGE], model: DRAFT.authoring_model_ref,
    preview: {definition: FLEET_DETAIL.definition, setupRevision: "1"}, savedSetupRevision: null});
  expect(client.getSnapshot().previousDrafts).toEqual([]);
  expect(fixture.mutations()).toEqual([]);
});

it("refreshes access readiness without marking an unsaved proposal as saved", async () => {
  fixture.current = {...DRAFT, linked_fleet_id: FLEET_ID,
    review: {...REVIEW, effective: {...REVIEW.effective, target_setup_revision: "1"}}};
  const client = await fixture.ready(true);
  await client.refreshSetup();
  fixture.fleet = {...FLEET_DETAIL, setup: {state: "ready", checks: []}};
  await client.refreshFleetSetup(true);
  expect(client.getSnapshot()).toMatchObject({draft: {id: DRAFT_ID, state: "editing"}, savedSetupRevision: null, fleet: {setup: {checks: []}}});
  expect(fixture.mutations()).toEqual([]);
});

// Value: history outages remain visible while the current setup and composer stay usable.
it("retains current authoring on a history-list failure and can retry", async () => {
  const client = await fixture.ready();
  let view: DraftView = {...client.getSnapshot(), fleet: FLEET_DETAIL};
  fixture.override = () => response({}, 503);
  const update = (patch: Partial<typeof view>) => {view = {...view, ...patch};};
  await restoreFleetHistory(BASE, view.fleet, new AbortController().signal, () => view, update);
  expect(view).toMatchObject({truncated: true, busy: false, inaccessible: false, error: null});
  fixture.override = ({path}) => path.endsWith("/drafts") ? response({items: cards(SAVED), next_cursor: null})
    : path.endsWith(`/drafts/${DRAFT_ID}`) ? response(SAVED) : undefined;
  await restoreFleetHistory(BASE, view.fleet, new AbortController().signal, () => view, update);
  expect(view.previousDrafts?.[0]?.id).toBe(DRAFT_ID);
  expect(fixture.mutations()).toEqual([]);
});

// Value: protects=late history responses cannot repopulate a different or cleared fleet page; fails_when=restoration ignores navigation or cancellation;
// why_new=existing navigation cases kept a fleet selected; seam=none; no_claim=browser transport cancellation (mocked HTTP).
it.each(["abort", "switch fleet", "clear fleet"])("discards a late history list after %s", async action => {
  const client = await fixture.ready();
  let view: DraftView = {...client.getSnapshot(), fleet: FLEET_DETAIL};
  const pending = Promise.withResolvers<Response>();
  fixture.override = () => pending.promise;
  const controller = new AbortController();
  const update = vi.fn();
  const restoring = restoreFleetHistory(BASE, view.fleet, controller.signal, () => view, update);
  if (action === "abort") controller.abort();
  else if (action === "clear fleet") view = {...view, fleet: null};
  else view = {...view, fleet: {...FLEET_DETAIL, id: NEXT_DRAFT}};
  pending.resolve(response({items: cards(SAVED), next_cursor: NEXT_DRAFT}));
  await restoring;
  expect(update).not.toHaveBeenCalled();
  expect(fixture.calls.filter(call => call.path.includes("/drafts/"))).toEqual([]);
});

it("marks a bounded history incomplete and excludes active, duplicate and unrelated conversations", async () => {
  const client = await fixture.ready();
  let view: DraftView = {...client.getSnapshot(), fleet: FLEET_DETAIL,
    draft: {...DRAFT, linked_fleet_id: FLEET_ID}, previousDrafts: [{id: NEXT_DRAFT, messages: [], cancelledThrough: "0"}]};
  fixture.override = () => response({items: [...cards(SAVED, {...SAVED, id: NEXT_DRAFT}),
    {...cards(SAVED)[0], id: FLEET_ID, linked_fleet_id: NEXT_DRAFT}], next_cursor: NEXT_DRAFT});
  await restoreFleetHistory(BASE, view.fleet, new AbortController().signal, () => view,
    patch => {view = {...view, ...patch};});
  expect(view.truncated).toBe(true);
  expect(view.previousDrafts).toHaveLength(1);
  expect(fixture.calls.at(-1)?.path).toBe(`${BASE}/drafts`);
  expect(fixture.mutations()).toEqual([]);
});

it.each(["wrong fleet", "running", "unavailable"])("omits an unsafe historical conversation: %s", async refusal => {
  const client = await fixture.ready();
  let view: DraftView = {...client.getSnapshot(), fleet: FLEET_DETAIL};
  fixture.override = ({path}) => {
    if (path.endsWith("/drafts")) return response({items: cards(SAVED), next_cursor: null});
    if (path.endsWith(`/drafts/${DRAFT_ID}`)) return refusal === "unavailable" ? response({}, 503)
      : response({...SAVED, linked_fleet_id: refusal === "wrong fleet" ? NEXT_DRAFT : FLEET_ID});
    if (path.endsWith("/messages")) return response({items: [{...MESSAGE, state: "running"}], next_cursor: null});
    return undefined;
  };
  await restoreFleetHistory(BASE, view.fleet, new AbortController().signal, () => view,
    patch => {view = {...view, ...patch};});
  expect(view).toMatchObject({truncated: true, busy: false});
  expect(view.previousDrafts ?? []).toEqual([]);
  expect(fixture.mutations()).toEqual([]);
});

it("does not replace messages added while an older conversation is loading", async () => {
  const client = await fixture.ready();
  let view: DraftView = {...client.getSnapshot(), fleet: FLEET_DETAIL};
  const pending = Promise.withResolvers<Response>();
  fixture.override = ({path}) => path.endsWith("/drafts") ? response({items: cards(SAVED), next_cursor: null})
    : path.endsWith(`/drafts/${DRAFT_ID}`) ? pending.promise : undefined;
  const restoring = restoreFleetHistory(BASE, view.fleet, new AbortController().signal, () => view,
    patch => {view = {...view, ...patch};});
  await vi.waitFor(() => expect(fixture.calls.some(call => call.path.endsWith(`/drafts/${DRAFT_ID}`))).toBe(true));
  view = {...view, messages: [{...MESSAGE, text: "A new question"}], previousDrafts: [{id: DRAFT_ID, messages: [], cancelledThrough: "0"}]};
  pending.resolve(response(SAVED));
  await restoring;
  expect(view.messages[0]?.text).toBe("A new question");
  expect(view.previousDrafts).toHaveLength(1);
});

// Value: protects=continuing a setup retains each restored conversation once; fails_when=a delayed revision read overwrites or duplicates restored history;
// why_new=the original merge case did not include the current draft already archived; seam=none; no_claim=server history ordering (mocked HTTP).
it.each([false, true])("merges restored history without duplicating an already archived current draft: %s", async archived => {
  const client = await fixture.ready();
  let view: DraftView = {...client.getSnapshot(), fleet: FLEET_DETAIL, draft: SAVED, messages: [MESSAGE]};
  const pending = Promise.withResolvers<Response>();
  fixture.override = () => pending.promise;
  const continuing = continueSavedDraft(BASE, () => view, new AbortController().signal);
  view = {...view, previousDrafts: [{id: NEXT_DRAFT, cancelledThrough: "0", messages: [{...MESSAGE, text: "Earlier chat"}]},
    ...(archived ? [{id: DRAFT_ID, cancelledThrough: "0", messages: [MESSAGE]}] : [])]};
  pending.resolve(response(FLEET_DETAIL));
  const next = await continuing;
  expect(next?.patch.previousDrafts?.map(draft => draft.id)).toEqual([NEXT_DRAFT, DRAFT_ID]);
  expect(next?.patch.previousDrafts?.[0]?.messages[0]?.text).toBe("Earlier chat");
  expect(fixture.mutations()).toEqual([]);
});

// Value: protects=no selected fleet means no private-history request; fails_when=history restoration fetches without a current owner;
// why_new=late-response tests always began with a selected fleet; seam=none; no_claim=server authorization (no HTTP request is expected).
it("does not request conversation history before a fleet is selected", async () => {
  const client = await fixture.ready();
  const before = fixture.calls.length;
  const view = client.getSnapshot();
  await restoreFleetHistory(BASE, null, new AbortController().signal, () => view, patch => {
    throw new Error(`Unexpected history update: ${Object.keys(patch).join(",")}`);
  });
  expect(fixture.calls).toHaveLength(before);
  expect(client.getSnapshot()).toBe(view);
});
