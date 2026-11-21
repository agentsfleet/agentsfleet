import { useLayoutEffect, useState } from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { CHAT_PROPS, chatRouter } from "./fixture";
import { WorkspaceChat } from "@/components/domain/setup/conversation/WorkspaceChat";
import { DraftClientFixture } from "@/tests/helpers/draft-client-fixture";
import { DRAFT_ID, FLEET_ID, REVIEW, REVISION_ID, response } from "@/tests/helpers/draft-fixtures";

const INSTRUCTIONS = "Investigate failures without changing production.";
const REFUSED = "The saved revision could not be completed";
const definition = {...REVIEW.effective.definition, instructions_md: INSTRUCTIONS};
let fixture: DraftClientFixture;
let failure: "none" | "refresh" | "refusal" | "lost-answer" | "followup";
let patched: boolean;
let failed: boolean;
let applyCount: number;
let editedDefinition: typeof definition;

beforeEach(() => {
  chatRouter.replace.mockClear();
  fixture = new DraftClientFixture(); fixture.mount();
  failure = "none"; patched = false; failed = false; applyCount = 0;
  editedDefinition = definition;
  fixture.current = {...fixture.current, linked_fleet_id: FLEET_ID,
    review: {...REVIEW, effective: {...REVIEW.effective, target_setup_revision: "1"}}};
  fixture.fleet = {...fixture.fleet, status: "active", definition: REVIEW.effective.definition, setup: {state: "ready", checks: []}};
  fixture.override = call => {
    if (call.method === "PATCH" && call.path.endsWith(`/drafts/${DRAFT_ID}`)) {
      patched = true;
      fixture.current = {...fixture.current, etag: '"draft-two"',
        candidate: {id: REVISION_ID, revision: "2", definition: editedDefinition, canonical_digest: "candidate-two", policy_semantics: REVIEW.policy_semantics},
        review: {...REVIEW, review_digest: "review-two", effective: {...REVIEW.effective, definition: editedDefinition, target_setup_revision: "1"}}};
      return response(fixture.current);
    }
    if (call.method === "GET" && !failed && (failure === "refresh" && patched && call.path.endsWith(`/drafts/${DRAFT_ID}`)
      || failure === "followup" && applyCount > 0 && call.path.endsWith(`/fleets/${FLEET_ID}`))) {
      failed = true; return response({user_message: REFUSED}, 503);
    }
    if (call.method === "POST" && call.path.endsWith("/revisions")) {
      applyCount++;
      if (failure === "refusal" && !failed) {failed = true; return response({user_message: REFUSED}, 409);}
      fixture.fleet = {...fixture.fleet, definition: editedDefinition, setup_revision: "2", setup_etag: '"setup-two"'};
      if (failure === "lost-answer" && !failed) {failed = true; return response({user_message: REFUSED}, 503);}
      return response({fleet_id: FLEET_ID, setup_revision: "2", setup_etag: fixture.fleet.setup_etag, etag: fixture.fleet.etag, schedules: []}, 201);
    }
  };
});

afterEach(() => {cleanup(); fixture.dispose(); chatRouter.search = "";});

function OpeningNavigation() {
  const [revision, setRevision] = useState("1");
  useLayoutEffect(() => {setRevision("2");}, []);
  return <WorkspaceChat {...CHAT_PROPS} source={{kind: "fleet", fleet_id: FLEET_ID, setup_revision: revision}} />;
}

// Value: protects=source navigation before chat initialization finishes;
// fails_when=an unhandled source pin is acknowledged during first render; seam=HTTP fixture; no_claim=browser navigation.
it("loads a revision selected before the initial chat surface mounts", async () => {
  const normal = fixture.override;
  const first = {...fixture.fleet};
  const latest = {...first, name: "Current deployment watch", setup_revision: "2", definition};
  let reads = 0;
  fixture.override = call => {
    if (call.method === "GET" && call.path.endsWith("/authoring-context")) return response({fleet: ++reads === 1 ? first : latest, conversations: [], activities: {items: [], next_cursor: null}, next_cursor: null});
    return normal(call);
  };
  render(<OpeningNavigation />);
  expect(await screen.findByRole("heading", {name: latest.name})).toBeTruthy();
  expect(screen.queryByRole("button", {name: "Edit instructions"})).toBeNull();
  expect(fixture.mutations()).toEqual([]);
});

// Value: protects=continuing a saved fleet's instructions and creator history;
// fails_when=an unchanged incoming revision resets a locally advanced session; seam=HTTP fixture; no_claim=durable writes.
it.each([false, true])("reopens saved instructions and creator history (restored edit: %s)", async restored => {
  chatRouter.search = "view=chat";
  const normal = fixture.override;
  if (restored) fixture.override = call => call.method === "GET" && call.path.endsWith("/authoring-context")
    ? response({fleet: fixture.fleet, conversations: [{draft: fixture.current, messages: [], messages_starting_after: null, truncated: false}],
      activities: {items: [], next_cursor: null}, next_cursor: null}) : normal(call);
  const page = render(<WorkspaceChat {...CHAT_PROPS} source={{kind: "fleet", fleet_id: FLEET_ID, setup_revision: "1"}} />);
  await screen.findByRole("heading", {name: fixture.fleet.name});
  const composer = await screen.findByRole("textbox");
  fireEvent.change(composer, {target: {value: "Keep this unsent follow-up."}});
  fireEvent.click(screen.getByRole("button", {name: "Save and publish"}));
  await screen.findByRole("link", {name: "Open fleet"});
  expect(fixture.fleet.setup_revision).toBe("2");
  expect(chatRouter.replace).toHaveBeenLastCalledWith(`/w/${CHAT_PROPS.workspace}/fleets/new?fleet=${FLEET_ID}&setup_revision=2`, {scroll: false});
  expect(composer).toHaveProperty("value", "Keep this unsent follow-up.");
  const writes = [...fixture.mutations()];
  page.rerender(<WorkspaceChat {...CHAT_PROPS} source={{kind: "fleet", fleet_id: FLEET_ID, setup_revision: "2"}} />);
  await act(async () => {await Promise.resolve();});
  expect(fixture.mutations()).toEqual(writes);

});

// Value: protects=recovery from a stale saved authoring URL; fails_when=Retry reuses its old revision;
// seam=HTTP fixture and router; no_claim=real browser reload.
it("retries the current revision when a reloaded authoring URL is stale", async () => {
  chatRouter.replace.mockClear();
  fixture.fleet = {...fixture.fleet, setup_revision: "2", definition};
  const page = render(<WorkspaceChat {...CHAT_PROPS} source={{kind: "fleet", fleet_id: FLEET_ID, setup_revision: "1"}} />);
  fireEvent.click(await screen.findByRole("button", {name: "Retry"}));
  await screen.findByRole("heading", {name: fixture.fleet.name});
  expect(screen.queryByText(/This fleet changed/)).toBeNull();
  expect(chatRouter.replace).toHaveBeenLastCalledWith(`/w/${CHAT_PROPS.workspace}/fleets/new?fleet=${FLEET_ID}&setup_revision=2`, {scroll: false});
  const redirects = chatRouter.replace.mock.calls.length;
  page.rerender(<WorkspaceChat {...CHAT_PROPS} source={{kind: "fleet", fleet_id: FLEET_ID, setup_revision: "1"}} />);
  expect(chatRouter.replace).toHaveBeenCalledTimes(redirects);
  expect(screen.getByText("Draft", {exact: true})).toBeTruthy();
  expect(fixture.mutations()).toEqual([]);
});

it("retains the authoring route when a save response does not advance the pinned setup", async () => {
  const normal = fixture.override;
  fixture.override = call => call.method === "POST" && call.path.endsWith("/revisions")
    ? response({fleet_id: FLEET_ID, setup_revision: "1", setup_etag: fixture.fleet.setup_etag, etag: fixture.fleet.etag, schedules: []}, 201)
    : normal(call);
  render(<WorkspaceChat {...CHAT_PROPS} source={{kind: "fleet", fleet_id: FLEET_ID, setup_revision: "1"}} />);
  await screen.findByRole("heading", {name: fixture.fleet.name});
  fireEvent.click(await screen.findByRole("button", {name: "Save and publish"}));
  await screen.findByRole("link", {name: "Open fleet"});
  expect(chatRouter.replace).not.toHaveBeenCalled();
  expect(fixture.mutations().filter(call => call.path.endsWith("/revisions"))).toHaveLength(1);
});

// Value: protects=finishing an accepted edit after partial failure;
// fails_when=rebasing clears Save or retry issues another edit; seam=HTTP fixture; no_claim=database idempotency.
it.each(["refresh", "refusal", "lost-answer", "followup"] as const)("finishes a partial save after %s without duplicating the edit", async mode => {
  failure = mode;
  const client = await fixture.ready(true);
  await client.edit({definition});
  const apply = async () => {await client.refreshSetup();await client.apply();};
  await expect(apply()).rejects.toMatchObject({message: REFUSED});
  expect(failed).toBe(true);
  const pending = mode === "lost-answer" || mode === "followup";
  expect(client.getSnapshot().pending).toBe(pending);
  if (pending) await client.retry();
  else await apply();
  expect(client.getSnapshot().fleet?.setup_revision).toBe("2");
  expect(client.getSnapshot().pending).toBe(false);
  expect(screen.queryByRole("alert")).toBeNull();
  expect(fixture.mutations().filter(call => call.method === "PATCH")).toHaveLength(1);
  const applies = fixture.mutations().filter(call => call.path.endsWith("/revisions"));
  expect(applies).toHaveLength(mode === "refresh" ? 1 : 2);
  expect(fixture.mutations().filter(call => call.path.endsWith("/fleets"))).toEqual([]);
  if (pending) {
    expect(applies[1]?.body).toEqual(applies[0]?.body);
    expect(applies[1]?.headers.get("Idempotency-Key")).toBe(applies[0]?.headers.get("Idempotency-Key"));
    expect(applies[0]?.headers.get("Idempotency-Key")).toBeTruthy();
    expect(applies[1]?.headers.get("If-Match")).toBe(applies[0]?.headers.get("If-Match"));
  }
});
