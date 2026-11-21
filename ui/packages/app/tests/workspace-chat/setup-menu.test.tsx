// Exercise the preserved Plugin tree with its UI switch explicitly enabled.
vi.mock("@/lib/plugin-ui", () => ({pluginsUiEnabled: () => true}));
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { ChatFixture, CHAT_PROPS, chatRouter } from "./fixture";
import { DraftChatHeading } from "@/components/domain/setup/conversation/DraftChatHeading";
import { WorkspaceChat } from "@/components/domain/setup/conversation/WorkspaceChat";
import { CHAT_PLACEHOLDER } from "@/components/domain/setup/chat-defaults";
import { DraftClient } from "@/lib/api/drafts";
import { DRAFT, FLEET, REVIEW } from "@/tests/helpers/draft-fixtures";

const MENU = "Draft actions";
const SAVE = "Save fleet";
let fixture: ChatFixture;
let client: DraftClient;
beforeEach(async () => {
  fixture = new ChatFixture(); fixture.mount();
  client = new DraftClient({...CHAT_PROPS, subject: "creator"});
  await client.initialize(); await client.resume(DRAFT.id);
});
afterEach(() => {cleanup(); client.dispose(); vi.unstubAllGlobals();});

it("starts a fresh setup from the empty heading action without writing a draft", () => {
  client.fresh();
  const fresh = vi.spyOn(client, "fresh");
  render(<DraftChatHeading workspace={CHAT_PROPS.workspace} client={client} view={client.getSnapshot()} />);
  fireEvent.click(screen.getByRole("button", {name: "New fleet"}));
  expect(fresh).toHaveBeenCalledOnce();
  expect(client.getSnapshot().draft).toBeNull();
  expect(fixture.writes()).toHaveLength(0);
});

it.each([false, true])("hides private actions without an accessible setup (inaccessible: %s)", (inaccessible) => {
  render(<DraftChatHeading workspace={CHAT_PROPS.workspace} client={client} view={{...client.getSnapshot(), draft: null, preview: null, inaccessible}} />);
  expect(screen.queryByRole("button", {name: MENU})).toBeNull();
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(fixture.writes()).toHaveLength(0);
});

it("uses the draft title and keeps save and instruction editing out of the heading", () => {
  const review = {...REVIEW, effective: {...REVIEW.effective, target_setup_revision: "1"}};
  render(<DraftChatHeading workspace={CHAT_PROPS.workspace} client={client} view={{...client.getSnapshot(), draft: {...DRAFT, linked_fleet_id: FLEET.id, review}, fleet: FLEET}} />);
  expect(screen.queryByRole("button", {name: "Edit instructions"})).toBeNull();
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(screen.getByText("Draft", {exact: true})).toBeTruthy();
  expect(screen.queryByRole("button", {name: MENU})).toBeNull();
  expect(screen.queryByRole("button", {name: SAVE})).toBeNull();
  expect(fixture.writes()).toHaveLength(0);
});

it("removes the save action when the current viewer loses access", () => {
  render(<DraftChatHeading workspace={CHAT_PROPS.workspace} client={client} view={{...client.getSnapshot(), inaccessible: true}} />);
  expect(screen.queryByRole("button", {name: SAVE})).toBeNull();
  expect(fixture.writes()).toHaveLength(0);
});

// Value: protects=opening the current fleet after activation; fails_when=the heading navigates to stale setup or writes a new revision;
// why_new=existing heading tests used no fleet or an installing fleet; seam=none; no_claim=server refresh success.
it("opens the current active fleet from the composer without writing a revision", async () => {
  render(<WorkspaceChat {...CHAT_PROPS} fleetPage source={{kind: "fleet", fleet_id: FLEET.id, setup_revision: "1"}} />);
  const open = await screen.findByRole("button", {name: "Open fleet"});
  expect(screen.getByRole("heading", {name: FLEET.name})).toBeTruthy();
  expect(screen.queryByRole("link", {name: "Edit setup"})).toBeNull();
  expect(screen.getByTestId("fleet-chat-footer").contains(open)).toBe(true);
  fireEvent.click(open);
  expect(chatRouter.refresh).toHaveBeenCalledOnce();
  expect(chatRouter.push).not.toHaveBeenCalled();
  expect(fixture.writes()).toEqual([]);
});

// Value: protects=publishing a source before any chat turn; fails_when=publication opens without a reviewable draft or saves a fleet;
// why_new=existing publish tests began with a persisted draft; seam=none; no_claim=durable publication or provider latency.
it("prepares a definition source for publication without saving or running the fleet", async () => {
  render(<WorkspaceChat {...CHAT_PROPS} source={{kind: "definition", definition: REVIEW.effective.definition}} />);
  const publish = await screen.findByRole("button", {name: "Save and publish"});
  expect(fixture.writes()).toHaveLength(0);
  fireEvent.click(publish);
  await screen.findByText("Setup published");
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(fixture.writes()).toHaveLength(2);
  expect(fixture.writes()[0]).toMatchObject({path: expect.stringMatching(/\/drafts$/), method: "POST",
    body: {source: {kind: "definition", definition: REVIEW.effective.definition}}});
  expect(fixture.saveAttempts).toBe(0);
});

it("loads setup inspection when the summary is opened while preserving the draft", async () => {
  render(<WorkspaceChat {...CHAT_PROPS} draft={DRAFT.id} />);
  const input = await screen.findByRole("textbox", {name: CHAT_PLACEHOLDER});
  fireEvent.change(input, {target: {value: "Preserve this unsent setup request"}});
  expect(await screen.findByRole("region", {name: "Repositories"})).toBeTruthy();
  expect(screen.getByRole("textbox", {name: CHAT_PLACEHOLDER, hidden: true})).toHaveProperty("value", "Preserve this unsent setup request");
});

it("keeps an unfinished private chat identifiable before its first recorded message", () => {
  render(<DraftChatHeading client={client} view={{...client.getSnapshot(), messages: [], draft: {...DRAFT, review: undefined, candidate: undefined}}}
    workspace={CHAT_PROPS.workspace} />);
  expect(screen.getByRole("heading", {name: DRAFT.title})).toBeTruthy();
  expect(screen.getByRole("button", {name: "Draft actions"})).toBeTruthy();
});

it("publishes an inspected saved-fleet source without saving another fleet or a setup revision", async () => {
  render(<WorkspaceChat {...CHAT_PROPS} source={{kind: "fleet", fleet_id: FLEET.id, setup_revision: "1"}} />);
  const publish = await screen.findByRole("button", {name: "Save and publish"});
  expect(screen.getByRole("link", {name: "Open fleet"}).getAttribute("href")).toContain(FLEET.id);
  fireEvent.click(publish);
  await screen.findByText("Setup published");
  expect(fixture.writes()).toHaveLength(2);
  expect(fixture.writes()[0]?.body).toMatchObject({source: {kind: "fleet", fleet_id: FLEET.id, setup_revision: "1"}});
  expect(fixture.writes()[1]?.path).toMatch(/\/plugins$/);
  expect(fixture.saveAttempts).toBe(0);
});
