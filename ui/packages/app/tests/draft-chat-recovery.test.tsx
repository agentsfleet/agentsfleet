import { useSyncExternalStore, type ReactNode } from "react";
import { useAui } from "@assistant-ui/react";
import { Button } from "@agentsfleet/design-system";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { DraftConversation } from "@/components/domain/chat/DraftConversation";
import { DraftClient } from "@/lib/api/drafts";
import { UNKNOWN_USAGE_REASON } from "@/lib/api/draft-recovery";
import { CHAT_PLACEHOLDER } from "@/components/domain/chat/chat-defaults";
import { ChatFixture, CHAT_PROPS } from "./workspace-chat/fixture";
import { DRAFT, MESSAGE, MODEL, response } from "./helpers/draft-fixtures";

let fixture: ChatFixture;
let client: DraftClient;
beforeEach(async () => {
  fixture = new ChatFixture(); fixture.mount();
  client = new DraftClient({...CHAT_PROPS, subject: "creator"});
  await client.initialize();
});
afterEach(() => {cleanup(); client.dispose(); vi.unstubAllGlobals();});
function Live({children}: {children?: ReactNode}) {
  const view = useSyncExternalStore(client.subscribe, client.getSnapshot, client.getSnapshot);
  return <DraftConversation workspace={CHAT_PROPS.workspace} client={client} view={view}>{children}</DraftConversation>;
}

function RuntimeStop() {
  const aui = useAui();
  return <Button onClick={() => { aui.thread.cancelRun(); }}>Cancel from assistant runtime</Button>;
}

it.each(["queued", "running"] as const)("chat_recovery_shows_stop_and_queued_state for %s without a durable assistant row", async (state) => {
  fixture.messages = [{...MESSAGE, state}];
  await client.resume(DRAFT.id); render(<Live />);
  const label = state === "queued" ? "Queued" : "Refining setup";
  expect(screen.getByRole("status", {name: label})).toBeTruthy();
  expect(screen.queryByRole("status", {name: state === "queued" ? "Refining setup" : "Queued"})).toBeNull();
  expect(screen.getByRole("button", {name: "Stop authoring"})).toHaveProperty("disabled", false);
});

it("keeps the composer open after a failure whose usage is unaccounted, and offers no Stop", async () => {
  fixture.messages = [{...MESSAGE, role: "assistant", state: "failed", text: undefined,
    failure: {reason: UNKNOWN_USAGE_REASON, error_code: "UZ-AUTHOR-015", recovery_action: "retry_message"}}];
  await client.resume(DRAFT.id); render(<Live />);
  expect(screen.getByRole("textbox", {name: CHAT_PLACEHOLDER})).toHaveProperty("disabled", false);
  expect(screen.queryByRole("button", {name: "Stop authoring"})).toBeNull();
  expect(screen.getByRole("log").textContent).toContain("Usage is still being reconciled");
});

it("persists cancellation requested through the assistant runtime", async () => {
  fixture.messages = [{...MESSAGE, state: "running"}];
  const base = fixture.fetch.getMockImplementation()!;
  fixture.fetch.mockImplementation(async (input, init) => {
    const path = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    if (path.endsWith("/cancellations")) {
      if (typeof init?.body !== "string") throw new Error("Expected cancellation JSON");
      expect(JSON.parse(init.body)).toEqual({through_sequence: "1"});
      fixture.draft = {...fixture.draft, cancelled_through: "1"};
      fixture.messages = [{...MESSAGE, state: "cancelled"}];
      return response({cancelled_through: "1", stopped: [{sequence: "1", state: "cancelled"}]});
    }
    return base(input, init);
  });
  await client.resume(DRAFT.id); render(<Live><RuntimeStop /></Live>);
  fireEvent.click(screen.getByRole("button", {name: "Cancel from assistant runtime"}));
  await waitFor(() => expect(client.getSnapshot().draft?.cancelled_through).toBe("1"));
  expect(screen.queryByRole("status", {name: "Refining setup"})).toBeNull();
  expect(screen.getByRole("textbox", {name: CHAT_PLACEHOLDER})).toHaveProperty("disabled", false);
});

it.each(["MiniMax M3.1 Flash (Free)", "GLM 5.3 Flash"])("displays the selected model's supplied label: %s", (label) => {
  render(<DraftConversation workspace={CHAT_PROPS.workspace} client={client} view={{...client.getSnapshot(), models: [{...MODEL, label}], model: MODEL.ref}} />);
  expect(screen.getByRole("combobox", {name: "Authoring model"}).textContent).toBe(label);
});

it("focuses New fleet ready for typing after its launcher was focused", async () => {
  render(<Button>Open setup</Button>);
  screen.getByRole("button", {name: "Open setup"}).focus();
  render(<Live />);
  const input = screen.getByRole("textbox", {name: CHAT_PLACEHOLDER});
  expect(document.activeElement).toBe(input);
  await userEvent.keyboard("Watch the nightly build");
  expect(input).toHaveProperty("value", "Watch the nightly build");
});

it("focuses a second New fleet click even when the current chat is already new", async () => {
  render(<Live><Button onClick={() => client.fresh()}>New fleet</Button></Live>);
  await userEvent.click(screen.getByRole("button", {name: "New fleet"}));
  expect(document.activeElement).toBe(screen.getByRole("textbox", {name: CHAT_PLACEHOLDER}));
});

it("focuses a resumed chat and New fleet after leaving it", async () => {
  await client.resume(DRAFT.id);
  render(<Live><Button onClick={() => client.fresh()}>New fleet</Button></Live>);
  const input = screen.getByRole("textbox", {name: CHAT_PLACEHOLDER});
  expect(document.activeElement).toBe(input);
  await userEvent.click(screen.getByRole("button", {name: "New fleet"}));
  expect(document.activeElement).toBe(input);
});

it("waits for the new composer to become ready before focusing", () => {
  const view = client.getSnapshot();
  const rendered = render(<DraftConversation workspace={CHAT_PROPS.workspace} client={client} view={{...view, busy: true}} />);
  const input = screen.getByRole("textbox", {name: CHAT_PLACEHOLDER});
  expect(document.activeElement).not.toBe(input);
  rendered.rerender(<DraftConversation workspace={CHAT_PROPS.workspace} client={client} view={view} />);
  expect(document.activeElement).toBe(input);
});

it("returns the caret after the first message and accepts the next without another click", async () => {
  render(<Live />);
  await userEvent.keyboard("Watch the nightly build{Enter}");
  const input = screen.getByRole("textbox", {name: CHAT_PLACEHOLDER});
  await waitFor(() => {
    expect(client.getSnapshot().draft?.id).toBe(DRAFT.id);
    expect(input).toHaveProperty("disabled", false);
    expect(document.activeElement).toBe(input);
  });
  await userEvent.keyboard("And report errors");
  expect(input).toHaveProperty("value", "And report errors");
});

it("leaves model selection focused when conversation updates arrive", async () => {
  await client.resume(DRAFT.id);
  const view = client.getSnapshot();
  const rendered = render(<DraftConversation workspace={CHAT_PROPS.workspace} client={client} view={view} />);
  const model = screen.getByRole("combobox", {name: "Authoring model"});
  model.focus();
  rendered.rerender(<DraftConversation workspace={CHAT_PROPS.workspace} client={client} view={{...view, messages: [...view.messages]}} />);
  expect(document.activeElement).toBe(model);
});

it("leaves another control focused when a composer mounts or becomes ready", async () => {
  const menu = render(<div role="menu"><Button role="menuitem">Create workspace</Button></div>);
  const create = screen.getByRole("menuitem", {name: "Create workspace"});
  create.focus();
  const initial = render(<Live />);
  expect(document.activeElement).toBe(create);
  initial.unmount(); menu.unmount();
  await client.resume(DRAFT.id);
  const view = client.getSnapshot();
  const rendered = render(<DraftConversation workspace={CHAT_PROPS.workspace} client={client} view={{...view, busy: true}}><Button>Inspect answer</Button></DraftConversation>);
  const inspect = screen.getByRole("button", {name: "Inspect answer"});
  inspect.focus();
  rendered.rerender(<DraftConversation workspace={CHAT_PROPS.workspace} client={client} view={view}><Button>Inspect answer</Button></DraftConversation>);
  expect(screen.getByRole("textbox", {name: CHAT_PLACEHOLDER})).toHaveProperty("disabled", false);
  expect(document.activeElement).toBe(inspect);
});

it("retries an unaccounted turn by resending it, without a Stop", async () => {
  const failure = {reason: UNKNOWN_USAGE_REASON, error_code: "UZ-AUTHOR-015", recovery_action: "retry_message"};
  fixture.messages = [{...MESSAGE, state: "failed", failure}, {...MESSAGE, role: "assistant", state: "failed", text: undefined, failure}];
  const base = fixture.fetch.getMockImplementation()!;
  const writes: string[] = [];
  fixture.fetch.mockImplementation(async (input, init) => {
    const path = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    if (path.endsWith("/cancellations")) throw new Error("An unaccounted turn needs no cancellation");
    if (path.endsWith("/messages") && init?.method === "POST") {
      writes.push("send");
      if (typeof init.body !== "string") throw new Error("Expected message JSON");
      expect(JSON.parse(init.body)).toMatchObject({message: MESSAGE.text});
      return response({status: "accepted", sequence: "2", replayed: false});
    }
    return base(input, init);
  });
  await client.resume(DRAFT.id); render(<Live />);
  fireEvent.click(screen.getByRole("button", {name: "Try again"}));
  await waitFor(() => expect(writes).toEqual(["send"]));
  await waitFor(() => expect(client.getSnapshot().busy).toBe(false));
  expect(client.getSnapshot().error).toBeNull();
});

it("retries the failed turn's request instead of a newer message or composer text", async () => {
  const failure = {reason: "provider_failed", error_code: "UZ-AUTHOR-014", recovery_action: "retry_message"};
  fixture.messages = [{...MESSAGE, state: "failed"}, {...MESSAGE, role: "assistant", state: "failed", text: undefined, failure},
    {...MESSAGE, sequence: "2", text: "A different request"}];
  const base = fixture.fetch.getMockImplementation()!;
  const sent: string[] = [];
  fixture.fetch.mockImplementation(async (input, init) => {
    const path = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    if (path.endsWith("/cancellations")) throw new Error("Known usage needs no cancellation");
    if (path.endsWith("/messages") && init?.method === "POST") {
      if (typeof init.body !== "string") throw new Error("Expected message JSON");
      sent.push(JSON.parse(init.body).message);
      return response({status: "accepted", sequence: "3", replayed: false});
    }
    return base(input, init);
  });
  await client.resume(DRAFT.id); render(<Live />);
  fireEvent.change(screen.getByRole("textbox", {name: CHAT_PLACEHOLDER}), {target: {value: "Unsent text"}});
  fireEvent.click(screen.getByRole("button", {name: "Try again"}));
  await waitFor(() => expect(sent).toEqual([MESSAGE.text]));
  expect(screen.getByRole("textbox", {name: CHAT_PLACEHOLDER})).toHaveProperty("value", "Unsent text");
});
