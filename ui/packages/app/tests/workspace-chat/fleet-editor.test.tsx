import type { DraftView } from "@/lib/api/draft-state";
import { useState } from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { Input } from "@agentsfleet/design-system";
import { ChatFixture, CHAT_PROPS, chatRouter } from "./fixture";
import { FleetSetupWorkspace, EditSetupButton } from "@/components/domain/chat/FleetSetupWorkspace";
import { useFleetStreams } from "@/components/domain/FleetStreamProvider";
import { EDIT_SETUP_PLACEHOLDER } from "@/components/domain/chat/chat-defaults";
import { DraftConversation } from "@/components/domain/chat/DraftConversation";
import { SetupInstructions } from "@/components/domain/chat/SetupInstructions";
import { DraftClient } from "@/lib/api/drafts";
import { FLEET_DETAIL, FLEET_ID, MESSAGE, MODEL, OPERATION_ID, REVIEW, response } from "@/tests/helpers/draft-fixtures";
import "@/components/domain/chat/WorkspaceChat";

let fixture: ChatFixture;
let saveStatus: number;
beforeEach(() => {
  fixture = new ChatFixture(); fixture.mount(); saveStatus = 200;
  fixture.draft = {...fixture.draft, linked_fleet_id: FLEET_ID, review: {...REVIEW, effective: {...REVIEW.effective, target_setup_revision: "1"}}};
  const base = fixture.fetch.getMockImplementation()!;
  fixture.fetch.mockImplementation(async (input, init) => {
    const path = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    if (path.endsWith(`/fleets/${FLEET_ID}/revisions`)) {
      fixture.calls.push({path, method: init?.method ?? "GET", body: typeof init?.body === "string" ? JSON.parse(init.body) : undefined, headers: new Headers(init?.headers)});
      return saveStatus === 200 ? response({fleet_id: FLEET_ID, setup_revision: "2", setup_etag: '"two"', etag: '"two"', schedules: []})
        : response({user_message: "Another editor saved a newer version."}, saveStatus);
    }
    if (path.endsWith(`/fleets/${FLEET_ID}`)) return response({...FLEET_DETAIL, status: "active"});
    return base(input, init);
  });
});
afterEach(() => {cleanup(); vi.unstubAllGlobals();});

function workspace(account: string | null = CHAT_PROPS.account, revision = "1", model = MODEL.ref) {
  return <FleetSetupWorkspace {...CHAT_PROPS} account={account} fleet={FLEET_ID} revision={revision} model={model} viewer="creator" header={<EditSetupButton />}>
    <Input aria-label="Fleet message" defaultValue="Runtime conversation stays here" />
  </FleetSetupWorkspace>;
}

function serveSavedSetup(read = () => response({...FLEET_DETAIL, setup_revision: "2", status: "active"})) {
  const base = fixture.fetch.getMockImplementation()!;
  fixture.fetch.mockImplementation(async (input, init) => {
    const path = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    return path.endsWith(`/fleets/${FLEET_ID}`) ? read() : base(input, init);
  });
}

async function edit() {
  fixture.auth.isLoaded = false;
  fixture.auth.userId = "";
  const view = render(workspace());
  const runtime = screen.getByLabelText("Fleet message");
  fireEvent.change(runtime, {target: {value: "Unsent runtime message"}});
  fireEvent.click(screen.getByRole("button", {name: "Edit setup"}));
  const panel = screen.getByRole("complementary", {name: "Edit fleet setup"});
  fixture.auth.isLoaded = true;
  fixture.auth.userId = "creator";
  view.rerender(workspace());
  expect(screen.getByRole("complementary", {name: "Edit fleet setup"})).toBe(panel);
  const composer = await within(panel).findByLabelText(EDIT_SETUP_PLACEHOLDER);
  await waitFor(() => expect(composer).toHaveProperty("disabled", false));
  return {view, panel, composer, runtime};
}

async function propose(composer: HTMLElement) {
  fireEvent.change(composer, {target: {value: "Notify Slack after the review"}});
  fireEvent.click(screen.getByRole("button", {name: "Send message"}));
  await screen.findByText("Your setup is ready.");
  return screen.getByRole("button", {name: "Save changes"});
}

it("preserves hidden manual setup edits when a saved model refreshes the fleet revision", async () => {
  const {view, panel, composer} = await edit();
  await propose(composer);
  fireEvent.click(screen.getByRole("button", {name: "Edit instructions manually"}));
  const input = screen.getByLabelText("Instructions");
  const unsaved = "Keep these manual instructions until I save them.";
  fireEvent.change(input, {target: {value: unsaved}});
  fireEvent.click(screen.getByRole("button", {name: "Close setup editor"}));
  view.rerender(workspace(CHAT_PROPS.account, "2", "model:new-runtime"));
  fireEvent.click(screen.getByRole("button", {name: "Edit setup"}));
  expect(screen.getByRole("complementary")).toBe(panel);
  expect(screen.getByLabelText("Instructions")).toBe(input);
  expect(input).toHaveProperty("value", unsaved);
  expect(screen.getByRole("button", {name: "Save changes"})).toHaveProperty("disabled", true);
  expect(fixture.calls.some(call => call.path.endsWith("/revisions"))).toBe(false);
});

it("keeps an unsent setup message and starts its draft from the current saved revision", async () => {
  const {view, panel, composer} = await edit();
  const unsent = "Keep this setup message after the model changes.";
  fireEvent.change(composer, {target: {value: unsent}});
  fireEvent.click(screen.getByRole("button", {name: "Close setup editor"}));
  serveSavedSetup();
  view.rerender(workspace(CHAT_PROPS.account, "2", "model:new-runtime"));
  fireEvent.click(screen.getByRole("button", {name: "Edit setup"}));
  expect(screen.getByRole("complementary")).toBe(panel);
  expect(await screen.findByLabelText(EDIT_SETUP_PLACEHOLDER)).toBe(composer);
  expect(composer).toHaveProperty("value", unsent);
  await waitFor(() => expect(composer).toHaveProperty("disabled", false));
  fireEvent.click(screen.getByRole("button", {name: "Send message"}));
  await screen.findByText("Your setup is ready.");
  expect(fixture.writes().find(call => call.path.endsWith("/drafts"))?.body).toMatchObject({
    source: {kind: "fleet", fleet_id: FLEET_ID, setup_revision: "2"}, message: unsent,
  });
});

it("opens chat first, pins Save changes, saves v2 and preserves the mounted fleet conversation", async () => {
  const {panel, composer, runtime} = await edit();
  expect(panel.textContent).toContain("Edit your fleet");
  expect(panel.textContent).toContain("Update its instructions and settings.");
  expect(within(panel).queryByRole("dialog")).toBeNull();
  expect(within(panel).queryByLabelText("Instructions")).toBeNull();
  expect(runtime).toHaveProperty("value", "Unsent runtime message");
  const save = await propose(composer);
  expect(save.closest("[aria-label='Save fleet setup']")).not.toBeNull();
  expect(panel.contains(save)).toBe(false);
  expect(screen.getAllByRole("button", {name: "Save changes"})).toHaveLength(1);
  fireEvent.click(save);
  await waitFor(() => expect(screen.queryByRole("complementary")).toBeNull());
  expect(screen.getByLabelText("Fleet message")).toBe(runtime);
  expect(runtime).toHaveProperty("value", "Unsent runtime message");
  expect(chatRouter.refresh).toHaveBeenCalledOnce();
  expect(chatRouter.push).not.toHaveBeenCalled();
  expect(document.activeElement).toBe(screen.getByRole("button", {name: "Edit setup"}));
  const revision = fixture.calls.find(call => call.path.endsWith("/revisions"));
  expect(revision?.body).toMatchObject({review_digest: REVIEW.review_digest});
  expect(revision?.headers.get("If-Match")).toBeTruthy();
});

it("retains the proposed change and runtime conversation when saving conflicts", async () => {
  saveStatus = 412;
  const {composer, runtime} = await edit();
  fireEvent.click(await propose(composer));
  await screen.findByText("Another editor saved a newer version.");
  expect(screen.getByRole("complementary")).toBeTruthy();
  expect(screen.getByText("Your setup is ready.")).toBeTruthy();
  expect(runtime).toHaveProperty("value", "Unsent runtime message");
  expect(chatRouter.refresh).not.toHaveBeenCalled();
});

it("lets a stale private draft discard manual edits and save a fresh proposal", async () => {
  const {view, composer, runtime} = await edit();
  await propose(composer);
  serveSavedSetup();
  view.rerender(workspace(CHAT_PROPS.account, "2", "model:new-runtime"));
  fireEvent.click(screen.getByRole("button", {name: "Save changes"}));
  await screen.findByText("This fleet changed. Open Edit from its current setup and try again.");
  expect(screen.getByText("Your setup is ready.")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", {name: "Edit instructions manually"}));
  fireEvent.change(screen.getByLabelText("Instructions"), {target: {value: "Discard these private manual edits when starting over."}});
  fireEvent.click(screen.getByRole("button", {name: "Start over from current setup"}));
  const currentComposer = await screen.findByLabelText(EDIT_SETUP_PLACEHOLDER);
  await waitFor(() => expect(currentComposer).toHaveProperty("disabled", false));
  expect(screen.queryByRole("alert")).toBeNull();
  expect(screen.queryByText("Your setup is ready.")).toBeNull();
  expect(screen.getByLabelText("Fleet message")).toBe(runtime);
  expect(runtime).toHaveProperty("value", "Unsent runtime message");
  expect(fixture.calls.some(call => call.path.endsWith("/revisions"))).toBe(false);
  expect(fixture.writes().some(call => call.method === "DELETE")).toBe(false);
  expect(await propose(currentComposer)).toHaveProperty("disabled", false);
});

it("keeps typed setup text through a failed current-source refresh and retry", async () => {
  const {view, composer} = await edit();
  const unsent = "Keep my private setup message through a failed refresh.";
  fireEvent.change(composer, {target: {value: unsent}});
  let unavailable = true;
  serveSavedSetup(() => unavailable ? response({user_message: "Setup read unavailable"}, 503)
    : response({...FLEET_DETAIL, setup_revision: "2", status: "active"}));
  view.rerender(workspace(CHAT_PROPS.account, "2", "model:new-runtime"));
  await screen.findByText("Setup read unavailable");
  expect(composer).toHaveProperty("value", unsent);
  unavailable = false;
  fireEvent.click(screen.getByRole("button", {name: "Retry"}));
  await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
  expect(await screen.findByLabelText(EDIT_SETUP_PLACEHOLDER)).toBe(composer);
  expect(composer).toHaveProperty("value", unsent);
  expect(fixture.writes()).toHaveLength(0);
});

it("keeps start-over disabled when a failed Stop leaves setup authoring running", async () => {
  fixture.messages = [{...MESSAGE, role: "assistant", text: undefined, state: "running"}];
  const base = fixture.fetch.getMockImplementation()!;
  fixture.fetch.mockImplementation(async (input, init) => {
    const path = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    return path.endsWith("/cancellations") ? response({user_message: "Stop is unavailable"}, 503) : base(input, init);
  });
  const {composer} = await edit();
  fireEvent.change(composer, {target: {value: "Review this setup"}});
  fireEvent.click(screen.getByRole("button", {name: "Send message"}));
  fireEvent.click(await screen.findByRole("button", {name: "Stop authoring"}));
  await screen.findByText("Stop is unavailable");
  expect(screen.getByRole("button", {name: "Start over from current setup"})).toHaveProperty("disabled", true);
});

it("protects unapplied manual instructions from Save changes and keeps them when folded", async () => {
  const {composer} = await edit();
  const save = await propose(composer);
  const toggle = screen.getByRole("button", {name: "Edit instructions manually"});
  fireEvent.click(toggle);
  const instructions = screen.getByLabelText("Instructions");
  const original = REVIEW.effective.definition.instructions_md;
  const edited = "Keep this unsaved instruction until I apply it.";
  fireEvent.change(instructions, {target: {value: edited}});
  expect(save).toHaveProperty("disabled", true);
  fireEvent.click(save);
  expect(fixture.calls.some(call => call.path.endsWith("/revisions"))).toBe(false);
  fireEvent.click(toggle);
  expect(screen.queryByRole("textbox", {name: "Instructions"})).toBeNull();
  expect(save).toHaveProperty("disabled", true);
  fireEvent.click(toggle);
  expect(screen.getByLabelText("Instructions")).toHaveProperty("value", edited);
  fireEvent.change(instructions, {target: {value: original}});
  await waitFor(() => expect(save).toHaveProperty("disabled", false));
});

it("reviews a manual change before its first chat message and enables explicit saving after applying it", async () => {
  await edit();
  fireEvent.click(screen.getByRole("button", {name: "Edit instructions manually"}));
  expect(screen.queryByRole("button", {name: "Review and publish options"})).toBeNull();
  const edited = "Inspect the nightly build and report only actionable failures.";
  fireEvent.change(screen.getByLabelText("Instructions"), {target: {value: edited}});
  expect(screen.getByRole("button", {name: "Save changes"})).toHaveProperty("disabled", true);
  const base = fixture.fetch.getMockImplementation()!;
  fixture.fetch.mockImplementation(async (input, init) => {
    if (init?.method === "PATCH") {
      const definition = {...fixture.draft.candidate!.definition, instructions_md: edited};
      fixture.draft = {...fixture.draft, candidate: {...fixture.draft.candidate!, definition},
        review: {...REVIEW, effective: {...REVIEW.effective, definition, target_setup_revision: "1"}}};
      return response(fixture.draft);
    }
    return base(input, init);
  });
  fireEvent.click(screen.getByRole("button", {name: "Update instructions"}));
  await waitFor(() => expect(screen.queryByLabelText("Instructions")).toBeNull());
  expect(fixture.draft.candidate?.definition.instructions_md).toBe(edited);
  const save = screen.getByRole("button", {name: "Save changes"});
  expect(save).toHaveProperty("disabled", false);
  expect(fixture.calls.some(call => call.path.endsWith("/revisions"))).toBe(false);
  fireEvent.click(save);
  await waitFor(() => expect(screen.queryByRole("complementary")).toBeNull());
  expect(fixture.calls.filter(call => call.path.endsWith("/revisions"))).toHaveLength(1);
});

it("keeps review and publication controls available only after the private draft has been reviewed", async () => {
  const {composer} = await edit();
  await propose(composer);
  fireEvent.click(screen.getByRole("button", {name: "Edit instructions manually"}));
  fireEvent.click(screen.getByRole("button", {name: "Review and publish options"}));
  fireEvent.click(screen.getByRole("button", {name: "Publish to Fleet library"}));
  expect(screen.getByRole("button", {name: "Publish new entry"})).toHaveProperty("disabled", false);
  expect(fixture.writes().some(call => call.path.endsWith("/fleet-libraries"))).toBe(false);
});

it("retains unapplied instructions and blocks saving when a newer setup answer arrives", async () => {
  const client = new DraftClient({...CHAT_PROPS, subject: "creator"});
  const current = {...client.getSnapshot(), model: MODEL.ref, draft: fixture.draft, messages: fixture.messages};
  const edited = "Keep my manual work while the assistant proposes another setup.";
  function Editor({view}: {view: DraftView}) {
    const [dirty, setDirty] = useState(false);
    return <DraftConversation client={client} view={view} instructionsDirty={dirty}
      instructions={<SetupInstructions client={client} view={view} onDirtyChange={setDirty} />} />;
  }
  const rendered = render(<Editor view={current} />);
  fireEvent.click(screen.getByRole("button", {name: "Edit instructions manually"}));
  const instructions = screen.getByLabelText("Instructions");
  fireEvent.change(instructions, {target: {value: edited}});
  expect(screen.getByRole("button", {name: "Save changes"})).toHaveProperty("disabled", true);
  const candidate = {...fixture.draft.candidate!, id: OPERATION_ID};
  rendered.rerender(<Editor view={{...current, draft: {...fixture.draft, candidate},
    messages: [...current.messages, {...MESSAGE, sequence: "2", role: "assistant", candidate_id: candidate.id, text: "A newer setup is ready."}]}} />);
  await screen.findByText("A newer setup is ready.");
  expect(screen.getByLabelText("Instructions")).toBe(instructions);
  expect(instructions).toHaveProperty("value", edited);
  expect(screen.getByRole("button", {name: "Save changes"})).toHaveProperty("disabled", true);
  expect(fixture.calls.some(call => call.path.endsWith("/revisions"))).toBe(false);
  rendered.unmount(); client.dispose();
});

it("closes with Escape, restores focus and discards private editor state on identity change", async () => {
  const {composer, runtime} = await edit();
  fireEvent.change(composer, {target: {value: "private unsent edit"}});
  fireEvent.keyDown(document, {key: "Escape"});
  expect(screen.queryByRole("complementary")).toBeNull();
  expect(document.activeElement).toBe(screen.getByRole("button", {name: "Edit setup"}));
  expect(screen.getByLabelText("Fleet message")).toBe(runtime);
  fireEvent.click(screen.getByRole("button", {name: "Edit setup"}));
  expect(await screen.findByLabelText(EDIT_SETUP_PLACEHOLDER)).toHaveProperty("value", "private unsent edit");
  cleanup();
  const view = render(workspace());
  fireEvent.click(screen.getByRole("button", {name: "Edit setup"}));
  view.rerender(workspace("different-account"));
  expect(screen.queryByRole("complementary")).toBeNull();
});

it("hides private setup after sign-out and disables editing without workspace access", () => {
  fixture.auth.userId = "";
  const view = render(workspace());
  expect(screen.queryByRole("button", {name: "Edit setup"})).toBeNull();
  fixture.auth.userId = "creator";
  view.rerender(workspace(null));
  expect(screen.getByRole("button", {name: "Edit setup"})).toHaveProperty("disabled", true);
});

it("requires an identity owner and disables a detached setup action", () => {
  render(<EditSetupButton />);
  expect(screen.getByRole("button", {name: "Edit setup"})).toHaveProperty("disabled", true);
  function UnownedChat() {useFleetStreams(); return null;}
  expect(() => render(<UnownedChat />)).toThrow("Fleet chat requires an identity-owned stream provider.");
});

it("lets nested controls consume Escape without closing the setup workspace", async () => {
  await edit();
  const escape = new KeyboardEvent("keydown", {key: "Escape", bubbles: true, cancelable: true});
  escape.preventDefault();
  fireEvent(window, escape);
  fireEvent.keyDown(window, {key: "ArrowDown"});
  expect(screen.getByRole("complementary", {name: "Edit fleet setup"})).toBeTruthy();
});

it("returns focus to its inline opener and closes safely if that control later disappears", async () => {
  const content = (showOpener: boolean) => <FleetSetupWorkspace {...CHAT_PROPS} fleet={FLEET_ID} revision="1" viewer="creator" header={<span>Support triage</span>}>
    {showOpener ? <EditSetupButton inline /> : null}
    <Input aria-label="Fleet message" defaultValue="Keep this unsent question" />
  </FleetSetupWorkspace>;
  const view = render(content(true));
  const opener = screen.getByRole("button", {name: "Edit fleet setup"});
  fireEvent.click(opener);
  await screen.findByLabelText(EDIT_SETUP_PLACEHOLDER);
  fireEvent.keyDown(window, {key: "Escape"});
  expect(document.activeElement).toBe(opener);
  fireEvent.click(opener);
  await screen.findByLabelText(EDIT_SETUP_PLACEHOLDER);
  view.rerender(content(false));
  fireEvent.keyDown(window, {key: "Escape"});
  expect(screen.queryByRole("complementary")).toBeNull();
  expect(screen.getByLabelText("Fleet message")).toHaveProperty("value", "Keep this unsent question");
});
