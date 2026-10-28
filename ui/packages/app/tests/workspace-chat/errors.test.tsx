import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { ChatFixture, CHAT_PROPS, chooseSetupAction } from "./fixture";
import { DraftClient } from "@/lib/api/drafts";
import { ChatModelAvailability } from "@/components/domain/chat/ChatAvailability";
import { WorkspaceChat } from "@/components/domain/chat/WorkspaceChat";
import { DraftSidebarProvider } from "@/components/layout/draft-sidebar-state";
import { DRAFT, FLEET_DETAIL, FLEET_ID, MODEL, response, WORKSPACE } from "@/tests/helpers/draft-fixtures";

let fixture: ChatFixture;
beforeEach(() => { fixture = new ChatFixture(); fixture.mount(); });
afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });

it("explains unavailable setup, preserves the lock, and offers reload or fleet navigation", async () => {
  fixture.fetch.mockResolvedValueOnce(response({}, 404));
  const reload = vi.spyOn(window.location, "reload").mockImplementation(() => undefined);
  render(<WorkspaceChat {...CHAT_PROPS} />);
  expect((await screen.findByRole("alert")).textContent).toContain("Fleet setup isn’t available");
  expect(screen.getByRole("link", {name: "Your fleets"}).getAttribute("href")).toBe(`/w/${WORKSPACE}/fleets`);
  expect(screen.queryByRole("button", {name: /Review pull requests/})).toBeNull();
  expect(screen.queryByRole("textbox")).toBeNull();
  fireEvent.click(screen.getByRole("button", {name: "Reload"}));
  expect(reload).toHaveBeenCalledOnce();
  expect(fixture.writes()).toHaveLength(0);
});

it("retries a failed initial read without claiming the workspace has no models", async () => {
  fixture.fetch.mockResolvedValueOnce(response({}, 503));
  render(<WorkspaceChat {...CHAT_PROPS} />);
  await screen.findByText("We couldn’t load your models and chats. Try again in a moment.");
  expect(screen.queryByText(/Add a model to this workspace/)).toBeNull();
  fireEvent.click(screen.getByRole("button", {name: "Retry"}));
  await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
  await waitFor(() => expect(screen.getByRole("button", {name: /Review pull requests/})).toHaveProperty("disabled", false));
  expect(fixture.writes()).toHaveLength(0);
});

it("reloads unavailable unfinished setups from their own recovery action without writing", async () => {
  fixture.fetch.mockResolvedValueOnce(response({}, 503));
  render(<DraftSidebarProvider><WorkspaceChat {...CHAT_PROPS} /></DraftSidebarProvider>);
  await screen.findByText(/Unfinished setups could not be loaded/);
  fireEvent.click(screen.getByRole("button", {name: "Try again"}));
  await waitFor(() => expect(screen.queryByText(/Unfinished setups could not be loaded/)).toBeNull());
  expect(screen.queryByText(/We couldn’t load your models and chats/)).toBeNull();
  fireEvent.change(screen.getByRole("textbox", {name: "Describe the work you want this fleet to do…"}), {target: {value: "Review the nightly build"}});
  expect(screen.getByRole("button", {name: "Send message"})).toHaveProperty("disabled", false);
  expect(fixture.writes()).toHaveLength(0);
});


it("shows an oversized message refusal inside the chat without creating a draft", async () => {
  vi.spyOn(console, "error").mockImplementation(() => undefined);
  render(<WorkspaceChat {...CHAT_PROPS} />);
  const input = await screen.findByRole("textbox", {name: "Describe the work you want this fleet to do…"});
  await waitFor(() => expect(screen.getByRole("button", {name: /Review pull requests/})).toHaveProperty("disabled", false));
  fireEvent.change(input, {target: {value: "🌱".repeat(8_193)}});
  fireEvent.click(screen.getByRole("button", {name: "Send message"}));
  expect((await screen.findByRole("alert")).textContent).toContain("Messages must contain 1 to 32,768 bytes of text.");
  expect(fixture.writes()).toEqual([]);
});


it("offers model settings when a successful list confirms there are no authoring models", async () => {
  const base = fixture.fetch.getMockImplementation();
  if (!base) throw new Error("Chat fetch fixture is not installed");
  fixture.fetch.mockImplementation((input, init) => typeof input === "string" && input.includes("/models?")
    ? Promise.resolve(response({items: [], next_cursor: null})) : base(input, init));
  render(<WorkspaceChat {...CHAT_PROPS} />);
  expect((await screen.findByRole("alert")).textContent).toContain("Create a model to start chatting");
  expect(screen.getByRole("alert").className).toContain("max-w-measure");
  expect(screen.getByRole("link", {name: "Create model"}).getAttribute("href")).toBe(`/w/${WORKSPACE}/settings/models`);
  expect(screen.getByRole("button", {name: "Send message"})).toHaveProperty("disabled", true);
});

it("explains configured but ineligible models and prevents an empty model menu", async () => {
  const base = fixture.fetch.getMockImplementation();
  if (!base) throw new Error("Chat fetch fixture is not installed");
  fixture.fetch.mockImplementation((input, init) => typeof input === "string" && input.includes("/models?")
    ? Promise.resolve(response({items: [{...MODEL, eligibility: {authoring: false, runtime: true}}], next_cursor: null})) : base(input, init));
  render(<WorkspaceChat {...CHAT_PROPS} />);
  const alert = await screen.findByRole("alert");
  expect(alert.textContent).toContain("No chat model is available");
  expect(alert.textContent).toContain("provider credentials and model configuration");
  expect(screen.queryByText("Create a model to start chatting")).toBeNull();
  expect(screen.getByRole("combobox")).toHaveProperty("disabled", true);
  expect(fixture.writes()).toEqual([]);
});

it("keeps an incomplete model search distinct from a workspace with no configured models", async () => {
  const base = fixture.fetch.getMockImplementation();
  if (!base) throw new Error("Chat fetch fixture is not installed");
  fixture.fetch.mockImplementation((input, init) => typeof input === "string" && input.includes("/models?")
    ? Promise.resolve(response({items: [], next_cursor: "next-model-page"})) : base(input, init));
  render(<WorkspaceChat {...CHAT_PROPS} />);
  expect((await screen.findByRole("alert")).textContent).toContain("Load more chat models");
  expect(screen.queryByText("Create a model to start chatting")).toBeNull();
  expect(within(screen.getByRole("alert")).getByRole("button", {name: /Load more models/})).toBeTruthy();
  const reads = fixture.fetch.mock.calls.length;
  fireEvent.click(within(screen.getByRole("alert")).getByRole("button", {name: /Load more models/}));
  await waitFor(() => expect(fixture.fetch.mock.calls.length).toBeGreaterThan(reads));
  expect(screen.queryByText("Create a model to start chatting")).toBeNull();
  expect(fixture.writes()).toEqual([]);
});

it.each(["loading", "unavailable"] as const)("does not claim there are no models while the list is %s", (listStatus) => {
  const client = new DraftClient({...CHAT_PROPS, subject: "creator"});
  render(<ChatModelAvailability client={client} view={{...client.getSnapshot(), listStatus}} workspace={WORKSPACE} />);
  expect(screen.queryByRole("alert")).toBeNull();
  expect(screen.queryByLabelText("Loading chat models") !== null).toBe(listStatus === "loading");
  client.dispose();
});


it("takes platform model administrators to the model library when no model is configured", () => {
  const client = new DraftClient({...CHAT_PROPS, subject: "creator"});
  render(<ChatModelAvailability client={client} view={{...client.getSnapshot(), listStatus: "ready"}} workspace={WORKSPACE} canManagePlatformModels />);
  expect(screen.getByRole("link", {name: "Create model"}).getAttribute("href")).toBe("/admin/models");
  expect(screen.queryByRole("button", {name: "Retry"})).toBeNull();
  client.dispose();
});

// Value: protects=recovery of an unavailable saved setup; fails_when=start-over leaves the composer blocked or writes a new draft;
// why_new=existing retry coverage began without a fleet source; seam=none; no_claim=backend availability or instruction persistence.
it("reopens the current fleet setup after an initial read fails without creating another draft", async () => {
  const base = fixture.fetch.getMockImplementation();
  if (!base) throw new Error("Chat fetch fixture is not installed");
  let reads = 0;
  const unavailable = "Current setup is temporarily unavailable";
  fixture.fetch.mockImplementation((input, init) => {
    const path = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    if (path.endsWith(`/fleets/${FLEET_ID}/authoring-context`)) {
      reads += 1;
      return Promise.resolve(reads === 1 ? response({user_message: unavailable}, 503) : response({fleet: FLEET_DETAIL,
        conversations: [], activities: {items: [], next_cursor: null}, next_cursor: null}));
    }
    return base(input, init);
  });
  render(<WorkspaceChat {...CHAT_PROPS} source={{kind: "fleet", fleet_id: FLEET_ID, setup_revision: "1"}} />);
  await screen.findByText(unavailable);
  expect(screen.queryByRole("textbox")).toBeNull();
  fireEvent.click(screen.getByRole("button", {name: "Start over from current setup"}));
  expect(await screen.findByRole("heading", {name: FLEET_DETAIL.name})).toBeTruthy();
  await waitFor(() => expect(screen.getByRole("textbox")).toHaveProperty("disabled", false));
  expect(screen.queryByText(unavailable)).toBeNull();
  expect(reads).toBe(2);
  expect(fixture.writes()).toEqual([]);
});

// Value: protects=selection of the latest saved setup; fails_when=revision navigation keeps editing the old fleet instructions;
// why_new=source identity deliberately excludes revision so navigation must refresh in place; seam=none; no_claim=server revision ordering.
it("loads the new saved revision when the same fleet source changes in place", async () => {
  const base = fixture.fetch.getMockImplementation();
  if (!base) throw new Error("Chat fetch fixture is not installed");
  let current = FLEET_DETAIL;
  fixture.fetch.mockImplementation((input, init) => {
    const path = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    return path.endsWith(`/fleets/${FLEET_ID}/authoring-context`) ? Promise.resolve(response({fleet: current,
      conversations: [], activities: {items: [], next_cursor: null}, next_cursor: null})) : base(input, init);
  });
  const page = render(<WorkspaceChat {...CHAT_PROPS} source={{kind: "fleet", fleet_id: FLEET_ID, setup_revision: "1"}} />);
  await screen.findByRole("heading", {name: current.name});
  const nextName = "Updated performance watch";
  const nextInstructions = "Check the current deployment only.";
  current = {...FLEET_DETAIL, name: nextName, setup_revision: "2", definition: {...FLEET_DETAIL.definition, instructions_md: nextInstructions}};
  page.rerender(<WorkspaceChat {...CHAT_PROPS} source={{kind: "fleet", fleet_id: FLEET_ID, setup_revision: "2"}} />);
  expect(await screen.findByRole("heading", {name: nextName})).toBeTruthy();
  expect(screen.getByText("Fleet setup · v2")).toBeTruthy();
  expect(screen.queryByRole("heading", {name: FLEET_DETAIL.name})).toBeNull();
  expect(screen.queryByRole("alert")).toBeNull();
  await chooseSetupAction("Instructions");
  expect(screen.getByRole("textbox", {name: "Instructions"})).toHaveProperty("value", nextInstructions);
  expect(fixture.writes()).toEqual([]);
});

// Value: protects=retrying a failed edit without losing unsent chat; fails_when=retry duplicates the mutation or strands the conversation;
// why_new=existing recovery tests retried initial lists or setup activation; seam=none; no_claim=successful rename persistence.
it("refreshes a draft after an edit refusal while retaining the unsent message", async () => {
  render(<WorkspaceChat {...CHAT_PROPS} draft={DRAFT.id} />);
  const input = await screen.findByRole("textbox", {name: "Describe the work you want this fleet to do…"});
  await waitFor(() => expect(input).toHaveProperty("disabled", false));
  const unsent = "Keep this follow-up question";
  fireEvent.change(input, {target: {value: unsent}});
  await chooseSetupAction("Rename chat");
  fireEvent.change(screen.getByLabelText("Chat name"), {target: {value: "Updated setup conversation"}});
  const refused = "Conversation name could not be saved";
  fixture.fetch.mockResolvedValueOnce(response({user_message: refused}, 503));
  fireEvent.click(screen.getByRole("button", {name: "Save name"}));
  await screen.findByText(refused);
  fireEvent.click(screen.getByRole("button", {name: "Retry"}));
  await waitFor(() => expect(screen.queryByText(refused)).toBeNull());
  expect(input).toHaveProperty("value", unsent);
  expect(input).toHaveProperty("disabled", false);
  expect(screen.getByRole("button", {name: "Save name"})).toHaveProperty("disabled", false);
  expect(fixture.saveAttempts).toBe(0);
  expect(fixture.fetch.mock.calls.filter(([, init]) => init?.method === "PATCH")).toHaveLength(1);
});
