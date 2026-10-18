import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { WorkspaceChat } from "@/components/domain/chat/WorkspaceChat";
import { ChatFixture, CHAT_PROPS } from "./fixture";
import { DRAFT, MESSAGE, response } from "@/tests/helpers/draft-fixtures";

const requestPath = (input: string | URL | Request): string => typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
let fixture: ChatFixture;
beforeEach(() => {fixture = new ChatFixture(); fixture.mount();});
afterEach(() => {cleanup(); vi.unstubAllGlobals();});

it("holds the loading state while resuming a chat and never renders the new-fleet entrance", async () => {
  const done = Promise.withResolvers<Response>();
  const base = fixture.fetch.getMockImplementation()!;
  fixture.fetch.mockImplementation(async (input, init) => requestPath(input).endsWith(`/drafts/${DRAFT.id}`) ? done.promise : base(input, init));
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
  const next = {...DRAFT, id: "0199a801-1000-7000-8000-00000000000a", title: "Morning brief"};
  const base = fixture.fetch.getMockImplementation()!;
  fixture.messages = [{...MESSAGE, text: "Prepare a morning brief"}];
  fixture.fetch.mockImplementation(async (input, init) => requestPath(input).endsWith(`/drafts/${next.id}`) ? done.promise : base(input, init));
  view.rerender(<WorkspaceChat {...CHAT_PROPS} draft={next.id} />);
  expect(screen.queryByText("Your setup is ready.")).toBeNull();
  expect(screen.queryByText("What should your fleet do?")).toBeNull();
  await act(async () => done.resolve(response(next)));
  await screen.findByRole("heading", {name: "Morning brief"});
});

it("renames a chat with a conditional patch and retains the editor on a refusal", async () => {
  render(<WorkspaceChat {...CHAT_PROPS} draft={DRAFT.id} />);
  fireEvent.click(await screen.findByRole("button", {name: "Rename setup chat"}));
  fireEvent.change(screen.getByLabelText("Chat name"), {target: {value: "Build repair"}});
  fixture.fetch.mockResolvedValueOnce(response({user_message: "This chat changed. Try again."}, 412));
  fireEvent.click(screen.getByRole("button", {name: "Save name"}));
  await screen.findByText("This chat changed. Try again.");
  expect(screen.getByLabelText("Chat name")).toHaveProperty("value", "Build repair");
  fixture.fetch.mockResolvedValueOnce(response({...DRAFT, title: "Build repair"}));
  fireEvent.click(screen.getByRole("button", {name: "Save name"}));
  await waitFor(() => expect(screen.queryByLabelText("Chat name")).toBeNull());
  expect(screen.getByRole("heading", {name: "Build repair"})).toBeTruthy();
  expect(fixture.writes().at(-1)?.headers.get("If-Match")).toBe(DRAFT.etag);
});

it("retries the selected chat after an initial read failure without showing a blank fleet", async () => {
  fixture.fetch.mockRejectedValueOnce(new TypeError("network unavailable"));
  render(<WorkspaceChat {...CHAT_PROPS} draft={DRAFT.id} />);
  fireEvent.click(await screen.findByRole("button", {name: "Retry"}));
  expect(screen.queryByText("What should your fleet do?")).toBeNull();
  await screen.findByText("Your setup is ready.");
  expect(screen.queryByText("What should your fleet do?")).toBeNull();
});
