import { useState, useSyncExternalStore } from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Alert } from "@agentsfleet/design-system";
import { DraftClient } from "@/lib/api/drafts";
import { SetupUnavailable } from "@/lib/api/draft-request";
import { DraftChatHeading } from "@/components/domain/chat/DraftChatHeading";
import { DraftConversation } from "@/components/domain/chat/DraftConversation";
import { FleetSetup } from "@/components/domain/chat/FleetSetup";
import { SetupStatusFlap } from "@/components/domain/chat/SetupStatusFlap";
import { ChatFixture, CHAT_PROPS, chatRouter, chooseSetupAction } from "./fixture";
import { DRAFT, FLEET, FLEET_ID, LIBRARY_CARD, PUBLICATION, REVIEW, WORKSPACE, response } from "@/tests/helpers/draft-fixtures";
import { DRAFT_STATE } from "@/lib/api/draft-types";

const approvals = vi.hoisted(() => ({list: vi.fn(), approve: vi.fn(), deny: vi.fn()}));
vi.mock("@/lib/actions/fleet-approvals", () => ({listApprovalsAction: approvals.list, approveApprovalAction: approvals.approve, denyApprovalAction: approvals.deny}));

let fixture: ChatFixture;
let client: DraftClient;
beforeEach(async () => {
  fixture = new ChatFixture(); fixture.mount();
  client = new DraftClient({...CHAT_PROPS, subject: "creator"});
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

function SetupConversation() {
  const view = useSyncExternalStore(client.subscribe, client.getSnapshot, client.getSnapshot);
  return <>{view.error ? <Alert variant="destructive">{view.error}</Alert> : null}
    <DraftConversation client={client} view={view} workspace={WORKSPACE} subject="creator">
      <FleetSetup client={client} view={view} workspace={WORKSPACE} />
    </DraftConversation></>;
}

it("discovers a newly opened approval after readiness refresh with unchanged setup history", async () => {
  const gate = {gate_id: "new-setup-gate", workspace_id: WORKSPACE, fleet_id: FLEET_ID, status: "pending",
    proposed_action: "Allow GitHub access", tool_name: "github", action_name: "access", blast_radius: "Selected repositories"};
  approvals.list.mockReset().mockResolvedValueOnce({ok: true, data: {items: [], next_cursor: null}})
    .mockResolvedValue({ok: true, data: {items: [gate], next_cursor: null}});
  fixture.draft = {...DRAFT, state: DRAFT_STATE.closed, linked_fleet_id: FLEET_ID};
  await client.resume(DRAFT.id);
  render(<SetupConversation />);
  await waitFor(() => expect(approvals.list).toHaveBeenCalledOnce());
  expect(screen.queryByRole("button", {name: "Approve"})).toBeNull();
  const previous = client.getSnapshot();
  await act(async () => client.refreshFleetSetup(true));
  expect(await screen.findByRole("button", {name: "Approve"})).toBeTruthy();
  expect(client.getSnapshot().activities).toEqual(previous.activities);
  expect(client.getSnapshot().focusRevision).toBe(previous.focusRevision);
  expect(approvals.list).toHaveBeenCalledTimes(2);
  expect(fixture.writes()).toEqual([]);
});

// Value: protects=approval-to-setup continuation; fails_when=approval leaves stale readiness or starts the fleet automatically;
// why_new=summary tests stop at the decision receipt; seam=network and Server Actions; no_claim=backend grant persistence.
it.each(["ready", "unavailable"])("keeps the decision in chat when the readiness read is %s", async result => {
  let approved = false;
  const gate = {gate_id: "setup-gate", workspace_id: WORKSPACE, fleet_id: FLEET_ID, status: "pending",
    proposed_action: "Allow GitHub access", tool_name: "github", action_name: "access", blast_radius: "Selected repositories"};
  approvals.list.mockResolvedValue({ok: true, data: {items: [gate], next_cursor: null}});
  approvals.approve.mockImplementation(async () => {
    approved = true;
    return {ok: true, data: {kind: "resolved", data: {gate_id: gate.gate_id, action_id: "github-access",
      outcome: "approved", resolved_at: "2026-11-12T10:00:00Z", resolved_by: "creator"}}};
  });
  fixture.draft = {...DRAFT, state: DRAFT_STATE.closed, linked_fleet_id: FLEET_ID};
  const normal = fixture.fetch.getMockImplementation()!;
  const fleetReads: string[] = [];
  fixture.fetch.mockImplementation((input, init) => {
    const path = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    if (!path.endsWith(`/fleets/${FLEET_ID}`)) return normal(input, init);
    fleetReads.push(init?.method ?? "GET");
    if (approved && result === "unavailable") return Promise.resolve(response({user_message: "Readiness could not be loaded"}, 503));
    return Promise.resolve(response({...FLEET, setup: {state: "waiting", checks: [
      {kind: "grant", subject: "github", state: approved ? "ready" : "pending"},
    ]}}));
  });
  await client.resume(DRAFT.id);
  render(<SetupConversation />);
  expect(screen.queryByRole("button", {name: "Finish setup"})).toBeNull();
  fireEvent.click(await screen.findByRole("button", {name: "Approve"}));
  await screen.findByText("Request approved.");
  if (result === "ready") {
    await waitFor(() => expect(screen.getByRole("button", {name: "Finish setup"})).toHaveProperty("disabled", false));
  } else {
    await screen.findByText("Readiness could not be loaded");
    expect(screen.queryByRole("button", {name: "Finish setup"})).toBeNull();
  }
  expect(fleetReads).toEqual(["GET", "GET"]);
  expect(fixture.writes()).toEqual([]);
  expect(chatRouter.push).not.toHaveBeenCalled();
  expect(chatRouter.replace).not.toHaveBeenCalled();
  expect(client.getSnapshot().fleet?.status).toBe("installing");
});

// Value: protects=approval during an older setup refresh; fails_when=the stale read suppresses the post-decision readiness read;
// why_new=the preceding cases have an idle client; seam=network and Server Actions; no_claim=backend grant persistence.
it("rereads readiness after an older in-flight read completes", async () => {
  const olderRead = Promise.withResolvers<Response>();
  let reads = 0;
  const gate = {gate_id: "setup-overlap", workspace_id: WORKSPACE, fleet_id: FLEET_ID, status: "pending",
    proposed_action: "Allow GitHub access", tool_name: "github", action_name: "access", blast_radius: "Selected repositories"};
  approvals.list.mockResolvedValue({ok: true, data: {items: [gate], next_cursor: null}});
  approvals.approve.mockResolvedValue({ok: true, data: {kind: "resolved", data: {gate_id: gate.gate_id,
    action_id: "github-access", outcome: "approved", resolved_at: "2026-11-12T10:00:00Z", resolved_by: "creator"}}});
  fixture.draft = {...DRAFT, state: DRAFT_STATE.closed, linked_fleet_id: FLEET_ID};
  const normal = fixture.fetch.getMockImplementation()!;
  const waiting = {...FLEET, setup: {state: "waiting", checks: [{kind: "grant", subject: "github", state: "pending"}]}};
  fixture.fetch.mockImplementation((input, init) => {
    const path = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    if (!path.endsWith(`/fleets/${FLEET_ID}`)) return normal(input, init);
    expect(init?.method ?? "GET").toBe("GET");
    reads++;
    if (reads === 2) return olderRead.promise;
    return Promise.resolve(response(reads === 1 ? waiting : {...waiting, setup: {state: "ready", checks: []}}));
  });
  await client.resume(DRAFT.id);
  render(<SetupConversation />);
  await act(async () => {void client.refreshFleetSetup(true);});
  expect(client.getSnapshot().busy).toBe(true);
  fireEvent.click(await screen.findByRole("button", {name: "Approve"}));
  await screen.findByText("Request approved.");
  expect(reads).toBe(2);
  await act(async () => olderRead.resolve(response(waiting)));
  await waitFor(() => expect(screen.getByRole("button", {name: "Finish setup"})).toHaveProperty("disabled", false));
  expect(reads).toBe(3);
  expect(screen.queryByRole("alert")).toBeNull();
  expect(client.getSnapshot().fleet?.status).toBe("installing");
  expect(fixture.writes()).toEqual([]);
});

// Value: protects=queued refresh ownership; fails_when=an old decision triggers a read after its client, fleet, user or access changes;
// why_new=the overlap regression keeps one scope; seam=network and Server Actions; no_claim=server authorization.
it.each(["client", "fleet", "subject", "access"])("discards the queued readiness read after changing %s", async changed => {
  const olderRead = Promise.withResolvers<Response>();
  let reads = 0;
  const gate = {gate_id: "scope-change", workspace_id: WORKSPACE, fleet_id: FLEET_ID, status: "pending",
    proposed_action: "Allow GitHub access", tool_name: "github", action_name: "access", blast_radius: "Selected repositories"};
  approvals.list.mockResolvedValue({ok: true, data: {items: [gate], next_cursor: null}});
  approvals.approve.mockResolvedValue({ok: true, data: {kind: "resolved", data: {gate_id: gate.gate_id,
    action_id: "github-access", outcome: "approved", resolved_at: "2026-11-12T10:00:00Z", resolved_by: "creator"}}});
  fixture.draft = {...DRAFT, state: DRAFT_STATE.closed, linked_fleet_id: FLEET_ID};
  const normal = fixture.fetch.getMockImplementation()!;
  fixture.fetch.mockImplementation((input, init) => {
    const path = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    if (!path.endsWith(`/fleets/${FLEET_ID}`)) return normal(input, init);
    reads++;
    return reads === 1 ? Promise.resolve(response(FLEET)) : olderRead.promise;
  });
  await client.resume(DRAFT.id);
  await act(async () => {void client.refreshFleetSetup(true);});
  const snapshot = client.getSnapshot();
  const replacement = new DraftClient({...CHAT_PROPS, subject: "creator"});
  const props = {client, view: snapshot, workspace: WORKSPACE, subject: "creator"};
  const rendered = render(<DraftConversation {...props} />);
  fireEvent.click(await screen.findByRole("button", {name: "Approve"}));
  await screen.findByText("Request approved.");
  // A render may precede the external store update; the native owner still blocks the read.
  rendered.rerender(<DraftConversation {...props} view={{...snapshot, busy: false}} />);
  expect(reads).toBe(2);
  rendered.rerender(<DraftConversation {...props} client={changed === "client" ? replacement : client}
    subject={changed === "subject" ? "another-user" : "creator"}
    view={{...snapshot, inaccessible: changed === "access", fleet: changed === "fleet" ? null : snapshot.fleet}} />);
  await act(async () => olderRead.resolve(response(FLEET)));
  rendered.rerender(<DraftConversation {...props} view={client.getSnapshot()} />);
  expect(reads).toBe(2);
  expect(fixture.writes()).toEqual([]);
  rendered.unmount();
  replacement.dispose();
});

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
  await chooseSetupAction("Publish plugin");
  fireEvent.change(await screen.findByLabelText("Plugin name"), {target: {value: "Checked recipe"}});
  expect(fixture.writes()).toEqual([]);
  fireEvent.click(screen.getByRole("button", {name: "Publish new entry"}));
  await screen.findByText("Setup published");
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(screen.getByRole("link", {name: "New fleet"}).getAttribute("href")).toBe(`/w/${WORKSPACE}/fleets/new?library=${PUBLICATION.library_id}&revision=${PUBLICATION.revision_id}`);
  expect(fixture.writes()[0]?.body).toMatchObject({label: "Checked recipe"});
  expect(fixture.saveAttempts).toBe(0);
});

it("requires an explicit owned-plugin selection when a resumed draft has no publication target", async () => {
  const normal = fixture.fetch.getMockImplementation()!;
  const writes: {path: string; init: RequestInit | undefined}[] = [];
  fixture.fetch.mockImplementation((input, init) => {
    const path = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    if (path.includes("/fleet-libraries/owned?")) return Promise.resolve(response({items: [{...LIBRARY_CARD, visibility: "tenant"}], next_cursor: null}));
    if (path.endsWith(`/fleet-libraries/${LIBRARY_CARD.id}`)) return Promise.resolve(response({etag: '"selected-version"'}));
    if (path.endsWith(`/fleet-libraries/${LIBRARY_CARD.id}/revisions`)) {
      writes.push({path, init});
      fixture.draft = {...fixture.draft, publication: {library_id: PUBLICATION.library_id, revision_id: PUBLICATION.revision_id, canonical_digest: fixture.draft.candidate?.canonical_digest ?? ""}};
      return Promise.resolve(response(PUBLICATION));
    }
    return normal(input, init);
  });
  render(<LiveSetup />); await chooseSetupAction("Publish plugin");
  expect(screen.queryByRole("button", {name: "Publish next revision"})).toBeNull();
  fireEvent.click(screen.getByRole("button", {name: "Update an existing plugin"}));
  fireEvent.click(await screen.findByRole("combobox", {name: "Plugin to update"}));
  fireEvent.click(await screen.findByRole("option", {name: LIBRARY_CARD.name}));
  expect(writes).toHaveLength(0);
  fireEvent.click(screen.getByRole("button", {name: "Publish next revision"}));
  await screen.findByText("Setup published");
  expect(writes).toHaveLength(1);
  expect(new Headers(writes[0]?.init?.headers).get("If-Match")).toBe('"selected-version"');
  expect(writes[0]?.init?.body).toBe(JSON.stringify({draft_candidate_id: DRAFT.candidate?.id}));
  expect(fixture.saveAttempts).toBe(0);
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
  await chooseSetupAction("Publish plugin");
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
