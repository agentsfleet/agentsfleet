import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { DraftClient } from "@/lib/api/drafts";
import { DefinitionEditor } from "@/components/domain/chat/DefinitionEditor";
import { DRAFT_STATE } from "@/lib/api/draft-types";
import { ChatFixture, CHAT_PROPS } from "./fixture";
import { DRAFT, FLEET, MODEL_REF, REVIEW, response } from "@/tests/helpers/draft-fixtures";

let fixture: ChatFixture;
let client: DraftClient;
beforeEach(() => {fixture = new ChatFixture(); fixture.mount(); client = new DraftClient({...CHAT_PROPS, subject: "creator"});});
afterEach(() => {cleanup(); client.dispose(); vi.unstubAllGlobals();});

it("offers a tall instructions editor with explicit saving and cancellation", async () => {
  await client.initialize(); await client.resume(DRAFT.id);
  const cancel = vi.fn();
  render(<DefinitionEditor client={client} view={client.getSnapshot()} onUpdated={vi.fn()} onCancel={cancel} />);
  expect(screen.getByLabelText("Instructions").getAttribute("rows")).toBe("18");
  expect(screen.queryByLabelText("Runtime config")).toBeNull();
  expect(screen.queryByRole("button", {name: "Advanced settings"})).toBeNull();
  expect(screen.queryByRole("button", {name: /Review/})).toBeNull();
  expect(screen.getByRole("button", {name: "Save changes"})).toHaveProperty("disabled", true);
  fireEvent.click(screen.getByRole("button", {name: "Cancel"}));
  expect(cancel).toHaveBeenCalledOnce();
  expect(fixture.writes()).toEqual([]);
});

it("updates instructions and name while preserving every configured tool, trigger and budget", async () => {
  await client.initialize(); await client.resume(DRAFT.id);
  const done = vi.fn();
  render(<DefinitionEditor client={client} view={client.getSnapshot()} onUpdated={done} />);
  fireEvent.change(screen.getByLabelText("Instructions"), {target: {value: "Investigate safely"}});
  fireEvent.change(screen.getByLabelText("Fleet name"), {target: {value: "safer-fleet"}});
  fireEvent.click(screen.getByRole("button", {name: "Save changes"}));
  await waitFor(() => expect(done).toHaveBeenCalledOnce());
  expect(fixture.writes()[0]?.body).toMatchObject({instance_name: "safer-fleet", config_ops: [],
    instruction_edits: [{start_byte: 0, end_byte: 22, text: "Investigate safely"}]});
  expect(client.getSnapshot().draft?.candidate?.definition.config).toEqual(REVIEW.effective.definition.config);
});

it.each([400, 403, 412, 503])("keeps manual text visible when saving refuses %i", async status => {
  await client.initialize(); await client.resume(DRAFT.id);
  const done = vi.fn();
  render(<DefinitionEditor client={client} view={client.getSnapshot()} onUpdated={done} />);
  fireEvent.change(screen.getByLabelText("Instructions"), {target: {value: "Keep my manual changes"}});
  fixture.fetch.mockResolvedValueOnce(response({user_message: "Instructions could not be saved"}, status));
  fireEvent.click(screen.getByRole("button", {name: "Save changes"}));
  expect((await screen.findByRole("alert")).textContent).toContain(status === 403 ? "Your account doesn’t have access here" : "Instructions could not be saved");
  expect(screen.getByLabelText("Instructions")).toHaveProperty("value", "Keep my manual changes");
  expect(done).not.toHaveBeenCalled();
});

it("does not create a fleet when editing an unsaved source preview", async () => {
  await client.initialize();
  client.fresh({kind: "definition", definition: REVIEW.effective.definition}); await client.refreshSetup();
  const done = vi.fn();
  render(<DefinitionEditor client={client} view={client.getSnapshot()} onUpdated={done} />);
  fireEvent.change(screen.getByLabelText("Instructions"), {target: {value: "An edited source preview"}});
  fireEvent.click(screen.getByRole("button", {name: "Save changes"}));
  await waitFor(() => expect(done).toHaveBeenCalledOnce());
  expect(client.getSnapshot().model).toBe(MODEL_REF);
  expect(fixture.saveAttempts).toBe(0);
});

it.each([undefined, "saving", "closed"])("blocks an uneditable candidate (%s)", state => {
  const draft = state === undefined ? {...DRAFT, candidate: undefined, review: undefined}
    : {...DRAFT, state: state === "saving" ? "saving" as const : "closed" as const};
  render(<DefinitionEditor client={client} view={{...client.getSnapshot(), draft}} onUpdated={vi.fn()} />);
  fireEvent.change(screen.getByLabelText("Instructions"), {target: {value: "A blocked change"}});
  expect(screen.getByRole("button", {name: "Save changes"})).toHaveProperty("disabled", true);
  expect(fixture.writes()).toEqual([]);
});

// Value: protects=continued editing of a saved setup; fails_when=a closed saved draft permanently locks its instructions;
// why_new=the closed-draft refusal test has no saved fleet; seam=none; no_claim=durable revision writes.
it("allows a closed saved setup to begin a new instruction edit without an immediate write", () => {
  const draft = {...DRAFT, state: "closed" as const, linked_fleet_id: FLEET.id};
  render(<DefinitionEditor client={client} view={{...client.getSnapshot(), draft, fleet: FLEET}} onUpdated={vi.fn()} />);
  fireEvent.change(screen.getByLabelText("Instructions"), {target: {value: "Continue improving this saved setup"}});
  expect(screen.getByRole("button", {name: "Save changes"})).toHaveProperty("disabled", false);
  expect(fixture.writes()).toEqual([]);
});

it.each([201, 409, 503])("applies manual changes to an existing fleet and preserves a refused edit (%i)", async status => {
  fixture.draft = {...DRAFT, linked_fleet_id: FLEET.id, state: DRAFT_STATE.editing,
    review: {...REVIEW, effective: {...REVIEW.effective, target_setup_revision: FLEET.setup_revision}}};
  await client.initialize(); await client.resume(DRAFT.id);
  const base = fixture.fetch.getMockImplementation();
  const refused = "Saved fleet update refused";
  let applied = false;
  fixture.fetch.mockImplementation(async (input, init) => {
    const path = new URL(typeof input === "string" ? input : input instanceof URL ? input.href : input.url, "http://app.test").pathname;
    if (path.endsWith("/revisions")) {
      applied = true;
      return response(status === 201 ? {fleet_id: FLEET.id, setup_revision: "2", setup_etag: FLEET.setup_etag, etag: FLEET.etag, schedules: []}
        : {user_message: refused}, status);
    }
    if (applied && status === 201 && path.endsWith(`/fleets/${FLEET.id}`)) return response({...FLEET, setup_revision: "2"});
    if (!base) throw new Error("Chat fixture is not mounted");
    return base(input, init);
  });
  const done = vi.fn();
  const text = "Improve the saved fleet safely";
  render(<DefinitionEditor client={client} view={client.getSnapshot()} onUpdated={done} />);
  fireEvent.change(screen.getByLabelText("Instructions"), {target: {value: text}});
  fireEvent.click(screen.getByRole("button", {name: "Save changes"}));
  if (status === 201) {
    await waitFor(() => expect(done).toHaveBeenCalledOnce());
    expect(client.getSnapshot().fleet?.setup_revision).toBe("2");
    expect(screen.queryByRole("alert")).toBeNull();
  } else {
    expect((await screen.findByRole("alert")).textContent).toContain(refused);
    expect(screen.getByLabelText("Instructions")).toHaveProperty("value", text);
    expect(done).not.toHaveBeenCalled();
  }
  expect(applied).toBe(true);
  expect(fixture.saveAttempts).toBe(0);
});
