import { useSyncExternalStore } from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Alert } from "@agentsfleet/design-system";
import { DraftClient } from "@/lib/api/drafts";
import { FleetSetup } from "@/components/domain/chat/FleetSetup";
import { ChatFixture, CHAT_PROPS } from "./fixture";
import { DRAFT, FLEET, FLEET_ID, PUBLICATION, REVIEW, WORKSPACE, response } from "@/tests/helpers/draft-fixtures";

let fixture: ChatFixture;
let client: DraftClient;
const stage = vi.fn();
beforeEach(async () => {
  fixture = new ChatFixture(); fixture.mount(); stage.mockReset(); stage.mockResolvedValue(PUBLICATION);
  client = new DraftClient({...CHAT_PROPS, subject: "creator"}, stage);
  await client.initialize(); await client.resume(DRAFT.id);
});
afterEach(() => {cleanup(); client.dispose(); vi.unstubAllGlobals();});
function LiveSetup() {
  const view = useSyncExternalStore(client.subscribe, client.getSnapshot, client.getSnapshot);
  return <>{view.error ? <Alert variant="destructive">{view.error}</Alert> : null}{view.notice ? <Alert>{view.notice}</Alert> : null}
    <FleetSetup client={client} view={view} workspace={WORKSPACE} /></>;
}

it("shows the task and saves directly without a review button or dialog", async () => {
  render(<LiveSetup />);
  expect(screen.getByText("Investigate failures")).toBeTruthy();
  expect(screen.getByText(/No automatic triggers/)).toBeTruthy();
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(screen.queryByRole("button", {name: /Review setup/i})).toBeNull();
  expect(fixture.writes()).toHaveLength(0);
  fireEvent.click(screen.getByRole("button", {name: "Save fleet"}));
  await screen.findByRole("heading", {name: "Fleet saved"});
  expect(screen.getByRole("link", {name: "Open fleet"}).getAttribute("href")).toBe(`/w/${WORKSPACE}/fleets/${FLEET_ID}`);
  expect(fixture.writes()).toHaveLength(1);
  expect(fixture.writes()[0]?.body).toMatchObject({review_digest: REVIEW.review_digest, start: {kind: "none"}});
});

it("starts a run only with an explicit message and action", async () => {
  render(<LiveSetup />);
  expect(screen.getByRole("button", {name: "Save and run now"})).toHaveProperty("disabled", true);
  fireEvent.change(screen.getByLabelText("Optional first run"), {target: {value: "   "}});
  expect(screen.getByRole("button", {name: "Save and run now"})).toHaveProperty("disabled", true);
  fireEvent.change(screen.getByLabelText("Optional first run"), {target: {value: " Check the build "}});
  fireEvent.click(screen.getByRole("button", {name: "Save and run now"}));
  await screen.findByRole("link", {name: "Open fleet"});
  expect(fixture.writes()[0]?.body).toMatchObject({start: {kind: "run_now", message: "Check the build"}});
});

it("keeps publishing explicit and separate from saving a fleet", async () => {
  render(<LiveSetup />);
  expect(screen.queryByRole("button", {name: "Publish new entry"})).toBeNull();
  fireEvent.click(screen.getByRole("button", {name: "Publish to Fleet library"}));
  fireEvent.change(await screen.findByLabelText("Library name"), {target: {value: "Checked recipe"}});
  fireEvent.click(screen.getByRole("button", {name: "Publish new entry"}));
  await screen.findByText(/Published Library revision/);
  expect(fixture.writes()[0]?.body).toMatchObject({label: "Checked recipe"});
  fireEvent.click(screen.getByRole("button", {name: "Stage platform entry"}));
  await screen.findByText(/is staged/);
  expect(stage).toHaveBeenCalledWith({workspace_id: WORKSPACE, draft_candidate_id: DRAFT.candidate?.id, label: "Checked recipe"}, expect.any(String));
  expect(fixture.saveAttempts).toBe(0);
});

it("lets editing be cancelled and updates the instructions before saving", async () => {
  render(<LiveSetup />);
  fireEvent.click(screen.getByRole("button", {name: "Edit instructions"}));
  expect(screen.getByLabelText("Instructions")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", {name: "Cancel editing"}));
  expect(screen.queryByLabelText("Instructions")).toBeNull();
  fireEvent.click(screen.getByRole("button", {name: "Edit instructions"}));
  fireEvent.change(screen.getByLabelText("Instructions"), {target: {value: "Changed instructions"}});
  fireEvent.click(screen.getByRole("button", {name: "Update instructions"}));
  await waitFor(() => expect(screen.queryByLabelText("Instructions")).toBeNull());
  expect(fixture.writes()[0]?.method).toBe("PATCH");
  expect(fixture.saveAttempts).toBe(0);
});

it("refreshes a changed candidate and refuses to save instructions the person has not seen", async () => {
  render(<LiveSetup />);
  fixture.draft = {...DRAFT, review: {...REVIEW, review_digest: "changed", effective: {...REVIEW.effective,
    definition: {...REVIEW.effective.definition, description: "Changed purpose"}}}};
  fireEvent.click(screen.getByRole("button", {name: "Save fleet"}));
  await screen.findByText(/The setup changed/);
  expect(screen.getByText("Changed purpose")).toBeTruthy();
  expect(fixture.saveAttempts).toBe(0);
  expect(fixture.writes()).toHaveLength(0);
});

it("keeps failed validation visible and does not create a fleet", async () => {
  render(<LiveSetup />);
  fixture.fetch.mockResolvedValueOnce(response({user_message: "Budget must be configured"}, 422));
  fireEvent.click(screen.getByRole("button", {name: "Save fleet"}));
  await screen.findByText("Budget must be configured");
  expect(screen.queryByRole("link", {name: "Open fleet"})).toBeNull();
  expect(fixture.saveAttempts).toBe(0);
});

it("shows unfinished candidate state and blocks controls during authoring", () => {
  const view = render(<FleetSetup client={client} view={{...client.getSnapshot(), draft: {...DRAFT, review: undefined}}} workspace={WORKSPACE} />);
  expect(screen.getByText(/Continue chatting to finish/)).toBeTruthy();
  expect(screen.queryByRole("button", {name: "Save fleet"})).toBeNull();
  view.rerender(<FleetSetup client={client} view={{...client.getSnapshot(), pending: true}} workspace={WORKSPACE} />);
  expect(screen.getByRole("button", {name: "Save fleet"})).toHaveProperty("disabled", true);
  expect(screen.getByRole("button", {name: "Edit instructions"})).toHaveProperty("disabled", true);
});

it("saves an improvement against the current setup tag and publishes another revision", async () => {
  fixture.draft = {...DRAFT, linked_fleet_id: FLEET_ID, publication: {...PUBLICATION, canonical_digest: "one"},
    review: {...REVIEW, effective: {...REVIEW.effective, target_setup_revision: "1"}}};
  const base = fixture.fetch.getMockImplementation()!;
  fixture.fetch.mockImplementation(async (input, init) => {
    const path = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    if (path.endsWith(`/fleets/${FLEET_ID}/revisions`)) {
      fixture.calls.push({path, method: init?.method ?? "GET", body: typeof init?.body === "string" ? JSON.parse(init.body) : undefined, headers: new Headers(init?.headers)});
      return response({fleet_id: FLEET_ID, setup_revision: "2", setup_etag: '"two"', etag: '"two"', schedules: []});
    }
    if (path.endsWith(`/fleet-libraries/${PUBLICATION.library_id}`)) return response({etag: PUBLICATION.etag});
    if (path.endsWith(`/fleet-libraries/${PUBLICATION.library_id}/revisions`)) return response(PUBLICATION);
    return base(input, init);
  });
  await client.resume(DRAFT.id); render(<LiveSetup />);
  fireEvent.click(screen.getByRole("button", {name: "Save changes"}));
  await screen.findByText(/Fleet setup updated/);
  expect(fixture.writes()[0]?.headers.get("If-Match")).toBe(FLEET.setup_etag);
  fireEvent.click(screen.getByRole("button", {name: "Publish to Fleet library"}));
  fireEvent.click(await screen.findByRole("button", {name: "Publish next revision"}));
  await screen.findByText(/Published Library revision/);
});

it("keeps an unsaved improvement editable when its validated Apply is refused", async () => {
  fixture.draft = {...DRAFT, linked_fleet_id: FLEET_ID,
    review: {...REVIEW, effective: {...REVIEW.effective, target_setup_revision: "1"}}};
  await client.resume(DRAFT.id);
  const base = fixture.fetch.getMockImplementation();
  if (!base) throw new Error("The chat fixture is not mounted");
  fixture.fetch.mockImplementation(async (input, init) => {
    const path = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    return path.endsWith(`/fleets/${FLEET_ID}/revisions`)
      ? response({user_message: "The saved fleet changed"}, 409) : base(input, init);
  });
  render(<LiveSetup />);
  fireEvent.click(screen.getByRole("button", {name: "Save changes"}));
  await screen.findByText("The saved fleet changed");
  expect(screen.queryByRole("heading", {name: "Fleet saved"})).toBeNull();
  expect(screen.getByRole("button", {name: "Save changes"})).toHaveProperty("disabled", false);
  expect(screen.getByRole("button", {name: "Edit instructions"})).toHaveProperty("disabled", false);
});
