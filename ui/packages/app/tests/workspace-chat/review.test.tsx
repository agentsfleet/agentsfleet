import { useSyncExternalStore } from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { DraftClient } from "@/lib/api/drafts";
import { ReviewSetup } from "@/components/domain/chat/ReviewSetup";
import { ChatFixture, CHAT_PROPS } from "./fixture";
import { DRAFT, FLEET, FLEET_ID, PUBLICATION, REVIEW, WORKSPACE, response } from "@/tests/helpers/draft-fixtures";

let fixture: ChatFixture;
let client: DraftClient;
const stage = vi.fn();
beforeEach(async () => {
  fixture = new ChatFixture(); fixture.mount(); stage.mockReset(); stage.mockResolvedValue(PUBLICATION);
  client = new DraftClient({...CHAT_PROPS, subject: "creator"}, stage);
  await client.initialize(); await client.resume(DRAFT.id); await client.review();
});
afterEach(() => {cleanup(); client.dispose(); vi.unstubAllGlobals();});
function LiveReview() {
  const view = useSyncExternalStore(client.subscribe, client.getSnapshot, client.getSnapshot);
  return <ReviewSetup client={client} view={view} workspace={WORKSPACE} open onOpenChange={() => {}} />;
}

it("publishes and stages only the displayed review and retains the returned notices", async () => {
  render(<LiveReview />);
  fireEvent.change(screen.getByLabelText("Library name"), {target: {value: "Checked recipe"}});
  fireEvent.click(screen.getByRole("button", {name: "Publish new entry"}));
  await screen.findByText(/Published Library revision/);
  expect(fixture.writes()[0]?.body).toMatchObject({label: "Checked recipe"});
  fireEvent.click(screen.getByRole("button", {name: "Stage platform entry"}));
  await screen.findByText(/is staged/);
  expect(stage).toHaveBeenCalledWith({workspace_id: WORKSPACE, draft_candidate_id: DRAFT.candidate?.id, label: "Checked recipe"}, expect.any(String));
});

it("saves an explicit first-run message and shows the saved fleet", async () => {
  render(<LiveReview />);
  expect(screen.getByRole("button", {name: "Save and run now"})).toHaveProperty("disabled", true);
  fireEvent.change(screen.getByLabelText("Optional first run"), {target: {value: "Check the build"}});
  fireEvent.click(screen.getByRole("button", {name: "Save and run now"}));
  await screen.findByRole("link", {name: "Open fleet"});
  expect(fixture.writes()[0]?.body).toMatchObject({start: {kind: "run_now", message: "Check the build"}});
});

it("explains why the proposed task needs checking without starting work", () => {
  render(<LiveReview />);
  expect(screen.getByText(/Check that this matches what you asked for/)).toBeTruthy();
  expect(screen.getByText("Investigate failures")).toBeTruthy();
  expect(screen.getByText(/No automatic triggers/)).toBeTruthy();
  expect(fixture.writes()).toHaveLength(0);
});

it("shows a saved fleet as a completed creation rather than another review", async () => {
  render(<LiveReview />);
  fireEvent.click(screen.getByRole("button", {name: /^Save fleet$/}));
  await screen.findByRole("heading", {name: "Fleet saved"});
  expect(screen.queryByRole("button", {name: "Edit setup"})).toBeNull();
  expect(screen.queryByRole("button", {name: "Refresh review"})).toBeNull();
  expect(screen.getByText(/You saved this fleet without requesting a first run/)).toBeTruthy();
  expect(screen.getByRole("link", {name: "Open fleet"}).getAttribute("href")).toBe(`/w/${WORKSPACE}/fleets/${FLEET_ID}`);
  expect(fixture.writes()).toHaveLength(1);
});

it("can edit, return to review, refresh, and recover an unsuccessful reviewed action", async () => {
  render(<LiveReview />);
  fireEvent.click(screen.getByRole("button", {name: "Edit setup"}));
  expect(screen.getByLabelText("Instructions")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", {name: "Show review"}));
  expect(screen.queryByLabelText("Instructions")).toBeNull();
  fireEvent.click(screen.getByRole("button", {name: "Refresh review"}));
  await waitFor(() => expect(screen.getByRole("button", {name: "Save fleet"})).toHaveProperty("disabled", false));
  fixture.fetch.mockResolvedValueOnce(response({}, 409));
  fireEvent.click(screen.getByRole("button", {name: "Save fleet"}));
  await screen.findByRole("alert");
  expect(client.getSnapshot().reviewed).toBeNull();
  expect(screen.queryByRole("link", {name: "Open fleet"})).toBeNull();
});

it("keeps a failed initial read distinct from blank editing and shows selected-source loading", () => {
  const empty = new DraftClient({...CHAT_PROPS, subject: "other"}).getSnapshot();
  const view = render(<ReviewSetup client={client} view={{...empty, busy: true}} workspace={WORKSPACE} open onOpenChange={() => {}} />);
  expect(screen.getByText("Loading selected setup…")).toBeTruthy();
  view.rerender(<ReviewSetup client={client} view={{...empty, error: "Read failed"}} workspace={WORKSPACE} open onOpenChange={() => {}} />);
  expect(screen.getByRole("alert").textContent).toBe("Read failed");
  expect(screen.queryByLabelText("Instructions")).toBeNull();
  view.rerender(<ReviewSetup client={client} view={empty} workspace={WORKSPACE} open onOpenChange={() => {}} />);
  expect(screen.getByLabelText("Instance name")).toHaveProperty("value", "my-fleet");
});

it("updates an edited candidate before returning to its review", async () => {
  render(<LiveReview />);
  fireEvent.click(screen.getByRole("button", {name: "Edit setup"}));
  fireEvent.change(screen.getByLabelText("Instructions"), {target: {value: "Changed instructions"}});
  fireEvent.click(screen.getByRole("button", {name: "Update draft and review"}));
  await screen.findByRole("button", {name: "Save fleet"});
  expect(screen.queryByLabelText("Instructions")).toBeNull();
  expect(fixture.writes()[0]?.method).toBe("PATCH");
});

it("continues an unfinished saved fleet through the retained save operation", async () => {
  const base = fixture.fetch.getMockImplementation()!;
  fixture.fetch.mockImplementation(async (input, init) => (typeof input === "string" ? input : input instanceof URL ? input.href : input.url).endsWith("/fleets") ? response(FLEET, 201) : base(input, init));
  await client.save(); render(<LiveReview />);
  fixture.fetch.mockImplementation(async (input, init) => (typeof input === "string" ? input : input instanceof URL ? input.href : input.url).endsWith("/fleets") ? response({...FLEET, status: "active"}, 201) : base(input, init));
  fireEvent.click(screen.getByRole("button", {name: "Continue setup"}));
  await screen.findByText(`${FLEET.name} is ready`);
  expect(screen.queryByRole("button", {name: "Continue setup"})).toBeNull();
});

it("applies a linked improvement and publishes a subsequent immutable revision", async () => {
  fixture.draft = {...DRAFT, linked_fleet_id: FLEET_ID, publication: {...PUBLICATION, canonical_digest: "one"},
    review: {...REVIEW, effective: {...REVIEW.effective, target_setup_revision: "1"}}};
  const base = fixture.fetch.getMockImplementation()!;
  fixture.fetch.mockImplementation(async (input, init) => {
    const path = (typeof input === "string" ? input : input instanceof URL ? input.href : input.url);
    if (path.endsWith(`/fleets/${FLEET_ID}/revisions`)) return response({fleet_id: FLEET_ID, setup_revision: "2", setup_etag: '"two"', etag: '"two"', schedules: []});
    if (path.endsWith(`/fleet-libraries/${PUBLICATION.library_id}`)) return response({etag: PUBLICATION.etag});
    if (path.endsWith(`/fleet-libraries/${PUBLICATION.library_id}/revisions`)) return response(PUBLICATION);
    return base(input, init);
  });
  await client.resume(DRAFT.id); await client.review(); render(<LiveReview />);
  fireEvent.click(screen.getByRole("button", {name: "Apply reviewed setup"}));
  await screen.findByText(/Fleet setup updated/);
  await act(async () => client.review());
  fireEvent.click(screen.getByRole("button", {name: "Publish next revision"}));
  await screen.findByText(/Published Library revision/);
});

it("retries an unregistered schedule on the fleet the review just saved", async () => {
  const owed = {schedule_id: "sched-9", fleet_id: FLEET_ID, sync: "syncing", last_error: null};
  const base = fixture.fetch.getMockImplementation()!;
  fixture.fetch.mockImplementation(async (input, init) => {
    const path = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    if (path.endsWith("/fleets") && init?.method === "POST") return response({...FLEET, status: "active", setup: {state: "ready", checks: []}, schedules: [owed]}, 201);
    if (path.endsWith("/schedules/sched-9/sync")) return response({...owed, sync: "synced"});
    return base(input, init);
  });
  render(<LiveReview />);
  fireEvent.click(screen.getByRole("button", {name: "Save fleet"}));
  fireEvent.click(await screen.findByRole("button", {name: "Retry registration"}));
  await screen.findByText(`${FLEET.name} is ready`);
});
