import { useState, useSyncExternalStore } from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Alert } from "@agentsfleet/design-system";
import { DraftClient } from "@/lib/api/drafts";
import { SetupUnavailable } from "@/lib/api/draft-request";
import { DraftChatHeading } from "@/components/domain/chat/DraftChatHeading";
import { FleetSetup } from "@/components/domain/chat/FleetSetup";
import { SetupStatusFlap } from "@/components/domain/chat/SetupStatusFlap";
import { ChatFixture, CHAT_PROPS, chooseSetupAction } from "./fixture";
import { DRAFT, FLEET_ID, PUBLICATION, REVIEW, WORKSPACE, response } from "@/tests/helpers/draft-fixtures";

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
  const [dirty, setDirty] = useState(false);
  return <>{view.error ? <Alert variant="destructive">{view.error}</Alert> : null}
    <DraftChatHeading client={client} view={view} workspace={WORKSPACE} onDirtyChange={setDirty} />
    <FleetSetup client={client} view={view} workspace={WORKSPACE} />
    <SetupStatusFlap view={view} workspace={WORKSPACE} dirty={dirty} />
    {dirty ? <output>Manual changes pending</output> : null}</>;
}

it("saves once from the heading without opening a review or starting a run", async () => {
  render(<LiveSetup />);
  expect(screen.getAllByRole("button", {name: "Save fleet"})).toHaveLength(1);
  expect(screen.queryByRole("button", {name: "Review setup"})).toBeNull();
  await chooseSetupAction("Save fleet");
  await screen.findByRole("heading", {name: "Setup saved · v1"});
  expect(fixture.writes()).toHaveLength(1);
  expect(fixture.writes()[0]?.body).toMatchObject({review_digest: REVIEW.review_digest, start: {kind: "none"}});
  expect(screen.getByRole("link", {name: "Open fleet"}).getAttribute("href")).toBe(`/w/${WORKSPACE}/fleets/${FLEET_ID}`);
});

it("keeps publishing in its own explicit menu action", async () => {
  render(<LiveSetup />);
  expect(screen.queryByRole("button", {name: "Publish new entry"})).toBeNull();
  await chooseSetupAction("Publish to Library");
  fireEvent.change(await screen.findByLabelText("Library name"), {target: {value: "Checked recipe"}});
  expect(fixture.writes()).toEqual([]);
  fireEvent.click(screen.getByRole("button", {name: "Publish new entry"}));
  await screen.findByText("Setup published");
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(screen.getByRole("link", {name: "New fleet"}).getAttribute("href")).toBe(`/w/${WORKSPACE}/fleets/new?library=${PUBLICATION.library_id}&revision=${PUBLICATION.revision_id}`);
  expect(fixture.writes()[0]?.body).toMatchObject({label: "Checked recipe"});
  expect(fixture.saveAttempts).toBe(0);
});

// Value: protects=platform publication refusal and retry; fails_when=refused staging claims success or saves a fleet;
// why_new=existing publication controls exercise workspace publication only; seam=none; no_claim=durable platform publication.
it("keeps platform staging failure visible and retries without saving a fleet", async () => {
  stage.mockRejectedValueOnce(new SetupUnavailable("Platform staging unavailable", 503));
  render(<LiveSetup />);
  await chooseSetupAction("Publish to Library");
  const button = await screen.findByRole("button", {name: "Stage platform entry"});
  fireEvent.click(button);
  await screen.findAllByText("Platform staging unavailable");
  expect(screen.queryByText(/is staged/)).toBeNull();
  expect(fixture.saveAttempts).toBe(0);
  expect(button).toHaveProperty("disabled", true);
  await act(async () => {await client.retry();});
  await screen.findAllByText(/is staged/);
  expect(stage).toHaveBeenCalledTimes(2);
  expect(fixture.writes()).toEqual([]);
});

// Value: protects=publication version conflicts; fails_when=a refused next revision reports success or saves the fleet instead;
// why_new=existing menu publication tests create a new Library entry; seam=none; no_claim=backend optimistic concurrency enforcement.
it("retains a refused next Library revision without claiming publication or changing the fleet", async () => {
  fixture.draft = {...DRAFT, publication: {library_id: PUBLICATION.library_id,
    revision_id: PUBLICATION.revision_id, canonical_digest: "previous-revision"}};
  await client.resume(DRAFT.id);
  const normal = fixture.fetch.getMockImplementation()!;
  fixture.fetch.mockImplementation((input, init) => {
    const path = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    if (path.endsWith(`/fleet-libraries/${PUBLICATION.library_id}`)) return Promise.resolve(response({etag: PUBLICATION.etag}));
    if (path.endsWith("/revisions")) return Promise.resolve(response({user_message: "Library revision changed"}, 412));
    return normal(input, init);
  });
  render(<LiveSetup />);
  await chooseSetupAction("Publish to Library");
  fireEvent.click(await screen.findByRole("button", {name: "Publish next revision"}));
  await screen.findAllByText("Library revision changed");
  expect(screen.queryByText(/Setup published/)).toBeNull();
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(client.getSnapshot().draft?.publication?.revision_id).toBe(PUBLICATION.revision_id);
  expect(fixture.saveAttempts).toBe(0);
});

it("cancels manual editing without saving or exposing advanced run options", async () => {
  render(<LiveSetup />); await chooseSetupAction("Instructions");
  fireEvent.change(screen.getByRole("textbox", {name: "Instructions"}), {target: {value: "Unsaved manual instructions"}});
  expect(await screen.findByText("Manual changes pending")).toBeTruthy();
  expect(screen.getByRole("button", {name: "Save fleet", hidden: true})).toHaveProperty("disabled", true);
  expect(screen.queryByRole("button", {name: "Review and run options"})).toBeNull();
  expect(screen.queryByRole("button", {name: "Advanced settings"})).toBeNull();
  fireEvent.click(screen.getByRole("button", {name: "Cancel"}));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(fixture.writes()).toEqual([]);
});

it("refuses saving a newer proposal that the person has not seen", async () => {
  render(<LiveSetup />);
  fixture.draft = {...DRAFT, review: {...REVIEW, review_digest: "changed"}};
  await chooseSetupAction("Save fleet");
  await screen.findByText(/The setup changed/);
  expect(fixture.saveAttempts).toBe(0);
  expect(fixture.writes()).toEqual([]);
});

it.each([403, 422, 503])("keeps a refused validation visible without creating a fleet (%i)", async status => {
  render(<LiveSetup />);
  fixture.fetch.mockResolvedValueOnce(response({user_message: "Setup could not be validated"}, status));
  await chooseSetupAction("Save fleet");
  await screen.findByText(status === 403 ? "Your account doesn’t have access here. Switch workspace or ask the workspace owner for access." : "Setup could not be validated");
  expect(fixture.saveAttempts).toBe(0);
});

it("keeps a refused improvement editable and never claims another revision was saved", async () => {
  fixture.draft = {...DRAFT, linked_fleet_id: FLEET_ID,
    review: {...REVIEW, effective: {...REVIEW.effective, target_setup_revision: "1"}}};
  await client.resume(DRAFT.id);
  const base = fixture.fetch.getMockImplementation();
  if (!base) throw new Error("Fixture is not mounted");
  fixture.fetch.mockImplementation((input, init) => {
    const path = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    return path.endsWith("/revisions") ? Promise.resolve(response({user_message: "The saved fleet changed"}, 409)) : base(input, init);
  });
  render(<LiveSetup />); await chooseSetupAction("Save changes");
  await screen.findByText("The saved fleet changed");
  expect(screen.queryByRole("heading", {name: "Setup saved · v2"})).toBeNull();
  expect(screen.getByRole("button", {name: "Save changes"})).toHaveProperty("disabled", false);
});
