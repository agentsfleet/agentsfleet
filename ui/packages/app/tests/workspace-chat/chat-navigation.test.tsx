import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { ChatFixture, CHAT_PROPS, chatRouter, chooseSetupAction, editChatTitle } from "./fixture";
import { DraftSidebarProvider } from "@/components/layout/draft-sidebar-state";
import { WorkspaceChat } from "@/components/domain/setup/conversation/WorkspaceChat";
import { DRAFT, REVIEW, MESSAGE, REVISION_ID, response } from "@/tests/helpers/draft-fixtures";

const requestPath = (input: string | URL | Request): string => typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
let fixture: ChatFixture;
beforeEach(() => {fixture = new ChatFixture(); fixture.mount();});
afterEach(() => {cleanup(); vi.unstubAllGlobals();});

it("holds the loading state while resuming a chat and never renders the new-fleet entrance", async () => {
  const done = Promise.withResolvers<Response>();
  const base = fixture.fetch.getMockImplementation();
  if (!base) throw new Error("Chat fetch fixture is not installed");
  fixture.fetch.mockImplementation(async (input, init) => requestPath(input).endsWith(`/drafts/${DRAFT.id}`) ? done.promise.then((value) => value.clone()) : base(input, init));
  render(<WorkspaceChat {...CHAT_PROPS} draft={DRAFT.id} />);
  await screen.findByLabelText("Loading setup chat");
  expect(screen.queryByText("What should your fleet do?")).toBeNull();
  await act(async () => done.resolve(response(DRAFT)));
  await screen.findByText("Your setup is ready.");
  expect(screen.queryByText("What should your fleet do?")).toBeNull();
});

it("drops the previous conversation immediately when a different chat is selected", async () => {
  const view = render(<WorkspaceChat {...CHAT_PROPS} draft={DRAFT.id} />);
  await screen.findByText("Your setup is ready.");
  const done = Promise.withResolvers<Response>();
  const next = {...DRAFT, id: REVISION_ID, title: "Morning brief", review: {...REVIEW, effective: {...REVIEW.effective, instance_name: "Morning brief"}}};
  const base = fixture.fetch.getMockImplementation();
  if (!base) throw new Error("Chat fetch fixture is not installed");
  fixture.messages = [{...MESSAGE, text: "Prepare a morning brief"}];
  fixture.fetch.mockImplementation(async (input, init) => requestPath(input).endsWith(`/drafts/${next.id}`) ? done.promise.then((value) => value.clone()) : base(input, init));
  view.rerender(<WorkspaceChat {...CHAT_PROPS} draft={next.id} />);
  expect(screen.queryByText("Your setup is ready.")).toBeNull();
  expect(screen.queryByText("What should your fleet do?")).toBeNull();
  await act(async () => done.resolve(response(next)));
  await screen.findByRole("heading", {name: "Morning brief"});
});

it("shows the owning cached conversation while its refresh is still in flight", async () => {
  const mounted = render(<WorkspaceChat {...CHAT_PROPS} draft={DRAFT.id} />);
  await screen.findByText("Your setup is ready.");
  fixture.draft = {...DRAFT, id: REVISION_ID, title: "Morning brief", review: {...REVIEW, effective: {...REVIEW.effective, instance_name: "Morning brief"}}};
  fixture.messages = [{...MESSAGE, text: "Prepare a morning brief"}];
  mounted.rerender(<WorkspaceChat {...CHAT_PROPS} draft={REVISION_ID} />);
  await screen.findByRole("heading", {name: "Morning brief"});
  const refreshing = Promise.withResolvers<Response>();
  const base = fixture.fetch.getMockImplementation();
  if (!base) throw new Error("Chat fetch fixture is not installed");
  fixture.draft = DRAFT;
  fixture.messages = [{...MESSAGE, role: "assistant", text: "Refreshed setup"}];
  fixture.fetch.mockImplementation(async (input, init) => requestPath(input).endsWith(`/drafts/${DRAFT.id}`)
    ? refreshing.promise.then(result => result.clone()) : base(input, init));
  mounted.rerender(<WorkspaceChat {...CHAT_PROPS} draft={DRAFT.id} />);
  expect(screen.getByText("Your setup is ready.")).toBeTruthy();
  expect(screen.queryByRole("heading", {name: "Morning brief"})).toBeNull();
  await act(async () => refreshing.resolve(response(DRAFT)));
  await screen.findByText("Refreshed setup");
  expect(screen.queryByText("Your setup is ready.")).toBeNull();
});

it("renames a chat with a conditional patch and retains the editor on a refusal", async () => {
  render(<WorkspaceChat {...CHAT_PROPS} draft={DRAFT.id} />);
  await editChatTitle();
  fireEvent.change(screen.getByLabelText("Chat name"), {target: {value: "Build repair"}});
  fixture.fetch.mockResolvedValueOnce(response({user_message: "This chat changed. Try again."}, 412));
  fireEvent.blur(screen.getByLabelText("Chat name"));
  await screen.findByText("Couldn’t complete this request");
  expect(screen.getByLabelText("Chat name")).toHaveProperty("value", "Build repair");
  fixture.fetch.mockResolvedValueOnce(response({...DRAFT, title: "Build repair"}));
  fireEvent.blur(screen.getByLabelText("Chat name"));
  await waitFor(() => expect(screen.queryByLabelText("Chat name")).toBeNull());
  expect(screen.getByRole("heading", {name: "Build repair"})).toBeTruthy();
  expect(within(screen.getByRole("region", {name: "Fleet setup"})).getByText(REVIEW.effective.instance_name)).toBeTruthy();
  expect(screen.getByRole("status")).toHaveProperty("textContent", "Chat name saved.");
  await editChatTitle();
  expect(screen.getByLabelText("Chat name")).toHaveProperty("value", "Build repair");
  const patch = fixture.fetch.mock.calls.at(-1)?.[1];
  expect(patch?.method).toBe("PATCH");
  expect(new Headers(patch?.headers).get("If-Match")).toBe(DRAFT.etag);
});

it.each([false, true])("keeps an outgoing title write scoped to its account (switch account: %s)", async (switchAccount) => {
  const pending = Promise.withResolvers<Response>();
  const request = Promise.withResolvers<AbortSignal>();
  const base = fixture.fetch.getMockImplementation()!;
  const updated = {...DRAFT, title: "Release checklist"};
  const next = {...DRAFT, id: REVISION_ID, title: "Support triage"};
  fixture.fetch.mockImplementation(async (input, init) => {
    if (init?.method === "PATCH") {
      expect(requestPath(input)).toContain(`/drafts/${DRAFT.id}`);
      request.resolve(init.signal!);
      return pending.promise;
    }
    if (requestPath(input).endsWith(`/drafts/${next.id}`)) return response(next);
    return base(input, init);
  });
  const mounted = render(<DraftSidebarProvider><WorkspaceChat {...CHAT_PROPS} draft={DRAFT.id} /></DraftSidebarProvider>);
  await editChatTitle();
  fireEvent.change(screen.getByLabelText("Chat name"), {target: {value: updated.title}});
  fireEvent.blur(screen.getByLabelText("Chat name"));
  const signal = await request.promise;
  mounted.rerender(<DraftSidebarProvider>
    <WorkspaceChat {...CHAT_PROPS} account={switchAccount ? "other-account" : CHAT_PROPS.account} draft={next.id} />
  </DraftSidebarProvider>);
  await screen.findByRole("heading", {name: next.title});
  expect(signal.aborted).toBe(switchAccount);
  if (!switchAccount) fixture.draft = updated;
  await act(async () => pending.resolve(response(updated)));
  expect(screen.getByRole("heading", {name: next.title})).toBeTruthy();
  mounted.rerender(<DraftSidebarProvider><WorkspaceChat {...CHAT_PROPS} draft={DRAFT.id} /></DraftSidebarProvider>);
  await screen.findByRole("heading", {name: switchAccount ? DRAFT.title : updated.title});
});

it.each([200, 412])("reopens a draft after its outgoing title write settles (HTTP %s)", async status => {
  const pending = Promise.withResolvers<Response>();
  const request = Promise.withResolvers<AbortSignal>();
  const base = fixture.fetch.getMockImplementation()!;
  const updated = {...DRAFT, title: "Release checklist", etag: '"renamed"'};
  fixture.fetch.mockImplementation((input, init) => {
    if (init?.method !== "PATCH") return base(input, init);
    request.resolve(init.signal!);
    return pending.promise;
  });
  const chat = <WorkspaceChat {...CHAT_PROPS} draft={DRAFT.id} />;
  const mounted = render(<DraftSidebarProvider>{chat}</DraftSidebarProvider>);
  await editChatTitle();
  fireEvent.change(screen.getByLabelText("Chat name"), {target: {value: updated.title}});
  fireEvent.blur(screen.getByLabelText("Chat name"));
  const signal = await request.promise;
  chatRouter.pathname = `/w/${CHAT_PROPS.workspace}/settings/models`;
  mounted.rerender(<DraftSidebarProvider><p>Models</p></DraftSidebarProvider>);
  expect(signal.aborted).toBe(false);
  const draftReads = () => fixture.calls.filter(call => call.method === "GET" && call.path.endsWith(`/drafts/${DRAFT.id}`)).length;
  const previousReads = draftReads();
  chatRouter.pathname = `/w/${CHAT_PROPS.workspace}/fleets/new`;
  mounted.rerender(<DraftSidebarProvider>{chat}</DraftSidebarProvider>);
  await screen.findByLabelText("Loading setup chat");
  expect(draftReads()).toBe(previousReads);
  if (status === 200) fixture.draft = updated;
  await act(async () => pending.resolve(response(status === 200 ? updated : {user_message: "This chat changed"}, status)));
  await screen.findByRole("heading", {name: status === 200 ? updated.title : DRAFT.title});
  expect(draftReads()).toBeGreaterThan(previousReads);
});

it.each(["workspace", "access"])("cancels a pending title write after losing %s", async change => {
  const pending = Promise.withResolvers<Response>();
  const request = Promise.withResolvers<AbortSignal>();
  const base = fixture.fetch.getMockImplementation()!;
  let denied = false;
  fixture.fetch.mockImplementation((input, init) => {
    if (init?.method === "PATCH") {
      request.resolve(init.signal!);
      return pending.promise;
    }
    if (denied && new URL(requestPath(input), "http://app.test").pathname.endsWith("/drafts")) {
      return Promise.resolve(response({user_message: "Access revoked"}, 403));
    }
    return base(input, init);
  });
  const chat = <WorkspaceChat {...CHAT_PROPS} draft={DRAFT.id} />;
  const mounted = render(<DraftSidebarProvider>{chat}</DraftSidebarProvider>);
  await editChatTitle();
  fireEvent.change(screen.getByLabelText("Chat name"), {target: {value: "Release checklist"}});
  fireEvent.blur(screen.getByLabelText("Chat name"));
  const signal = await request.promise;
  const reads = () => fixture.calls.filter(call => call.method === "GET" && call.path.endsWith(`/drafts/${DRAFT.id}`)).length;
  const previousReads = reads();
  if (change === "workspace") chatRouter.pathname = `/w/${REVISION_ID}/settings/models`;
  mounted.rerender(<DraftSidebarProvider><p>Models</p></DraftSidebarProvider>);

  if (change === "access") {
    expect(signal.aborted).toBe(false);
    denied = true;
    mounted.rerender(<DraftSidebarProvider>{chat}</DraftSidebarProvider>);
    await screen.findByRole("button", {name: "Reload"});
  }
  expect(signal.aborted).toBe(true);
  await act(async () => pending.resolve(response({...DRAFT, title: "Release checklist"})));
  expect(reads()).toBe(previousReads);
});

it("keeps a pending title write when a different chat was deleted", async () => {
  const pending = Promise.withResolvers<Response>();
  const request = Promise.withResolvers<AbortSignal>();
  const base = fixture.fetch.getMockImplementation()!;
  fixture.fetch.mockImplementation((input, init) => {
    if (init?.method === "PATCH") {
      request.resolve(init.signal!);
      return pending.promise;
    }
    if (requestPath(input).endsWith(`/drafts/${REVISION_ID}`)) return Promise.resolve(response({user_message: "Draft not found"}, 404));
    return base(input, init);
  });
  const view = (id: string) => <DraftSidebarProvider><WorkspaceChat {...CHAT_PROPS} draft={id} /></DraftSidebarProvider>;
  const mounted = render(view(DRAFT.id));
  const updated = {...DRAFT, title: "Release checklist"};
  await editChatTitle();
  fireEvent.change(screen.getByLabelText("Chat name"), {target: {value: updated.title}});
  fireEvent.blur(screen.getByLabelText("Chat name"));
  const signal = await request.promise;

  mounted.rerender(view(REVISION_ID));
  await screen.findByRole("button", {name: "Reload"});
  expect(signal.aborted).toBe(false);

  mounted.rerender(view(DRAFT.id));
  fixture.draft = updated;
  await act(async () => pending.resolve(response(updated)));
  await screen.findByRole("heading", {name: updated.title});
});

it("retries the selected chat after an initial read failure without showing a blank fleet", async () => {
  const base = fixture.fetch.getMockImplementation();
  if (!base) throw new Error("Chat fetch fixture is not installed");
  let failDraft = true;
  fixture.fetch.mockImplementation((input, init) => {
    if (failDraft && typeof input === "string" && input.endsWith(`/drafts/${DRAFT.id}`)) {
      failDraft = false;
      return Promise.reject(new TypeError("network unavailable"));
    }
    return base(input, init);
  });
  render(<WorkspaceChat {...CHAT_PROPS} draft={DRAFT.id} />);
  fireEvent.click(await screen.findByRole("button", {name: "Retry"}));
  expect(screen.queryByText("What should your fleet do?")).toBeNull();
  await screen.findByText("Your setup is ready.");
  expect(screen.queryByText("What should your fleet do?")).toBeNull();
});

it("cancels a name change without saving and reopens with the saved name", async () => {
  render(<WorkspaceChat {...CHAT_PROPS} draft={DRAFT.id} />);
  await editChatTitle();
  fireEvent.change(screen.getByLabelText("Chat name"), {target: {value: "Abandoned name"}});
  fireEvent.keyDown(screen.getByLabelText("Chat name"), {key: "Escape"});
  expect(screen.queryByLabelText("Chat name")).toBeNull();
  await editChatTitle();
  expect(screen.getByLabelText("Chat name")).toHaveProperty("value", DRAFT.title);
  expect(fixture.writes()).toHaveLength(0);
});

it("opens a pinned setup directly and cancels initialization when navigation leaves it", async () => {
  const view = render(<WorkspaceChat {...CHAT_PROPS} source={{kind: "definition", definition: DRAFT.candidate!.definition}} />);
  await screen.findByRole("heading", {name: "repairer"});
  expect(screen.queryByText("What should your fleet do?")).toBeNull();
  view.unmount();
  const pending = Promise.withResolvers<Response>();
  fixture.fetch.mockReturnValueOnce(pending.promise);
  const next = render(<WorkspaceChat {...CHAT_PROPS} draft={DRAFT.id} />);
  next.unmount();
  await act(async () => pending.resolve(response({items: [], next_cursor: null})));
  expect(screen.queryByText("Your setup is ready.")).toBeNull();
});
