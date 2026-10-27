import type { DraftClient } from "@/lib/api/drafts";
import { useLayoutEffect, useState, useSyncExternalStore } from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { CHAT_PROPS, chooseSetupAction } from "./fixture";
import { DefinitionEditor } from "@/components/domain/chat/DefinitionEditor";
import { WorkspaceChat } from "@/components/domain/chat/WorkspaceChat";
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

// Value: protects=editing again after a definite refusal; fails_when=retry drops newer text;
// seam=HTTP fixture; no_claim=durable revision history.
it("saves newer manual instructions after a definite Apply refusal", async () => {
  failure = "refusal";
  const client = await fixture.ready(true);
  const done = vi.fn();
  render(<LiveEditor client={client} done={done} dirty={vi.fn()} />);
  fireEvent.change(screen.getByLabelText("Instructions"), {target: {value: INSTRUCTIONS}});
  fireEvent.click(screen.getByRole("button", {name: "Save changes"}));
  await screen.findByText(REFUSED);
  const newer = "Ask the owner before repairing a production failure.";
  editedDefinition = {...definition, instructions_md: newer};
  fireEvent.change(screen.getByLabelText("Instructions"), {target: {value: newer}});
  fireEvent.click(screen.getByRole("button", {name: "Save changes"}));
  await waitFor(() => expect(done).toHaveBeenCalledOnce());
  const patches = fixture.mutations().filter(call => call.method === "PATCH");
  expect(patches).toHaveLength(2);
  expect(patches[1]?.body).toMatchObject({instruction_edits: [{text: newer}]});
  expect(client.getSnapshot().fleet?.definition?.instructions_md).toBe(newer);
  expect(applyCount).toBe(2);
});
afterEach(() => {cleanup(); fixture.dispose();});

function LiveEditor({client, done, dirty}: {client: DraftClient; done: () => void; dirty: (value: boolean) => void}) {
  const view = useSyncExternalStore(client.subscribe, client.getSnapshot, client.getSnapshot);
  return <DefinitionEditor client={client} view={view} onUpdated={done} onDirtyChange={dirty} />;
}

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
    if (call.method === "GET" && call.path.endsWith(`/fleets/${FLEET_ID}`)) return response(++reads === 1 ? first : latest);
    return normal(call);
  };
  render(<OpeningNavigation />);
  expect(await screen.findByRole("heading", {name: latest.name})).toBeTruthy();
  await chooseSetupAction("Instructions");
  expect(screen.getByRole("textbox", {name: "Instructions"})).toHaveProperty("value", INSTRUCTIONS);
  expect(fixture.mutations()).toEqual([]);
});

// Value: protects=continuing a saved fleet's instructions and creator history;
// fails_when=an unchanged incoming revision resets a locally advanced session; seam=HTTP fixture; no_claim=durable writes.
it.each([false, true])("reopens saved instructions and creator history (restored edit: %s)", async restored => {
  const normal = fixture.override;
  if (restored) fixture.override = call => call.method === "GET" && call.path.endsWith("/drafts") && call.search.includes("linked_fleet_id=")
    ? response({items: [{...fixture.current, updated_at: "1970-01-01T00:00:00Z", candidate_revision: 1, setup_revision: 1, publication_revision: null}], next_cursor: null}) : normal(call);
  const page = render(<WorkspaceChat {...CHAT_PROPS} source={{kind: "fleet", fleet_id: FLEET_ID, setup_revision: "1"}} />);
  await screen.findByRole("heading", {name: fixture.fleet.name});
  await chooseSetupAction("Instructions");
  fireEvent.change(screen.getByRole("textbox", {name: "Instructions"}), {target: {value: INSTRUCTIONS}});
  fireEvent.click(screen.getByRole("button", {name: "Save changes"}));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(fixture.fleet.setup_revision).toBe("2");
  const writes = [...fixture.mutations()];
  await chooseSetupAction("Instructions");
  await act(async () => {await Promise.resolve();});
  expect(await screen.findByRole("textbox", {name: "Instructions"})).toHaveProperty("value", INSTRUCTIONS);
  expect(screen.queryByText(/This fleet changed/)).toBeNull();
  expect(screen.getByText("Investigate failures")).toBeTruthy();
  expect(fixture.mutations()).toEqual(writes);
  const reads = fixture.calls.length;
  page.rerender(<WorkspaceChat {...CHAT_PROPS} source={{kind: "fleet", fleet_id: FLEET_ID, setup_revision: "2"}} />);
  await act(async () => {await Promise.resolve();});
  expect(fixture.calls).toHaveLength(reads);
});

// Value: protects=finishing an accepted edit after partial failure;
// fails_when=rebasing clears Save or retry issues another edit; seam=HTTP fixture; no_claim=database idempotency.
it.each(["refresh", "refusal", "lost-answer", "followup"] as const)("finishes a partial save after %s without duplicating the edit", async mode => {
  failure = mode;
  const client = await fixture.ready(true);
  const done = vi.fn(); const dirty = vi.fn();
  render(<LiveEditor client={client} done={done} dirty={dirty} />);
  fireEvent.change(screen.getByLabelText("Instructions"), {target: {value: INSTRUCTIONS}});
  fireEvent.click(screen.getByRole("button", {name: "Save changes"}));
  expect((await screen.findByRole("alert")).textContent).toContain(REFUSED);
  expect(failed).toBe(true);
  expect(done).not.toHaveBeenCalled();
  expect(dirty).toHaveBeenLastCalledWith(true);
  expect(screen.getByLabelText("Instructions")).toHaveProperty("value", INSTRUCTIONS);
  const pending = mode === "lost-answer" || mode === "followup";
  expect(screen.getByLabelText("Instructions")).toHaveProperty("disabled", pending);
  expect(screen.getByLabelText("Fleet name")).toHaveProperty("disabled", pending);
  const retry = screen.getByRole("button", {name: pending ? "Retry save" : "Save changes"});
  expect(retry).toHaveProperty("disabled", false);
  fireEvent.click(retry);
  await waitFor(() => {
    expect(done).toHaveBeenCalledOnce();
    expect(dirty).toHaveBeenLastCalledWith(false);
  });
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
