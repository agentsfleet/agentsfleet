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
  return <DraftConversation client={client} view={view}>{children}</DraftConversation>;
}

function RuntimeStop() {
  const aui = useAui();
  return <Button onClick={() => { aui.thread.cancelRun(); }}>Cancel from assistant runtime</Button>;
}

it.each(["queued", "running"] as const)("chat_recovery_shows_stop_and_queued_state for %s without a durable assistant row", async (state) => {
  fixture.messages = [{...MESSAGE, state}];
  await client.resume(DRAFT.id); render(<Live />);
  const label = state === "queued" ? "Queued" : "Working";
  expect(screen.getByRole("status", {name: label})).toBeTruthy();
  expect(screen.queryByRole("status", {name: state === "queued" ? "Working" : "Queued"})).toBeNull();
  expect(screen.getByRole("button", {name: "Stop authoring"})).toHaveProperty("disabled", false);
});

it("lets Stop unblock unknown usage with queued text while retaining the original failure", async () => {
  const unknown = {...MESSAGE, role: "assistant" as const, state: "failed" as const, text: undefined,
    failure: {reason: UNKNOWN_USAGE_REASON, error_code: "UZ-AUTHOR-015", recovery_action: "await_accounting"}};
  fixture.messages = [unknown, {...MESSAGE, sequence: "2", state: "queued"}];
  const base = fixture.fetch.getMockImplementation()!;
  fixture.fetch.mockImplementation(async (input, init) => {
    const path = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    if (path.endsWith("/cancellations")) {
      if (typeof init?.body !== "string") throw new Error("Expected cancellation JSON");
      expect(JSON.parse(init.body)).toEqual({through_sequence: "2"});
      fixture.draft = {...fixture.draft, cancelled_through: "2"};
      fixture.messages = [unknown, {...MESSAGE, sequence: "2", state: "cancelled"}];
      return response({cancelled_through: "2", stopped: [{sequence: "2", state: "cancelled"}]});
    }
    return base(input, init);
  });
  await client.resume(DRAFT.id); render(<Live />);
  expect(screen.getByRole("textbox", {name: CHAT_PLACEHOLDER})).toHaveProperty("disabled", true);
  expect(screen.getByRole("log").textContent).toContain("Stop this turn");
  fireEvent.click(screen.getByRole("button", {name: "Stop authoring"}));
  await waitFor(() => expect(screen.queryByRole("button", {name: "Stop authoring"})).toBeNull());
  expect(screen.getByRole("log").textContent).toContain("earlier usage is still being reconciled");
  expect(screen.queryByRole("status", {name: "Queued"})).toBeNull();
  expect(screen.queryByRole("status", {name: "Working"})).toBeNull();
  expect(screen.getByRole("textbox", {name: CHAT_PLACEHOLDER})).toHaveProperty("disabled", false);
});

it("offers Stop for a failed unknown turn even when nothing is running", async () => {
  fixture.messages = [{...MESSAGE, role: "assistant", state: "failed", text: undefined,
    failure: {reason: UNKNOWN_USAGE_REASON, error_code: "UZ-AUTHOR-015", recovery_action: "await_accounting"}}];
  await client.resume(DRAFT.id); render(<Live />);
  expect(screen.getByRole("button", {name: "Stop authoring"})).toHaveProperty("disabled", false);
  expect(screen.queryByRole("status", {name: "Working"})).toBeNull();
});

it("keeps unknown usage blocked after a failed Stop and permits another Stop", async () => {
  fixture.messages = [{...MESSAGE, role: "assistant", state: "failed", text: undefined,
    failure: {reason: UNKNOWN_USAGE_REASON, error_code: "UZ-AUTHOR-015", recovery_action: "await_accounting"}}];
  const base = fixture.fetch.getMockImplementation()!;
  let cancellations = 0;
  fixture.fetch.mockImplementation(async (input, init) => {
    const path = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    if (path.endsWith("/cancellations")) {
      cancellations++;
      if (cancellations === 1) return response({user_message: "Stop could not be saved."}, 503);
      fixture.draft = {...fixture.draft, cancelled_through: "1"};
      return response({cancelled_through: "1", stopped: []});
    }
    return base(input, init);
  });
  await client.resume(DRAFT.id); render(<Live />);
  fireEvent.click(screen.getByRole("button", {name: "Stop authoring"}));
  await waitFor(() => expect(client.getSnapshot().error).toBe("Stop could not be saved."));
  expect(client.getSnapshot().draft?.cancelled_through).toBe("0");
  expect(screen.getByRole("log").textContent).toContain("Stop this turn");
  expect(screen.getByRole("textbox", {name: CHAT_PLACEHOLDER})).toHaveProperty("disabled", true);
  fireEvent.click(screen.getByRole("button", {name: "Stop authoring"}));
  await waitFor(() => expect(screen.queryByRole("button", {name: "Stop authoring"})).toBeNull());
  expect(cancellations).toBe(2);
  expect(client.getSnapshot().error).toBeNull();
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
  expect(screen.queryByRole("status", {name: "Working"})).toBeNull();
  expect(screen.getByRole("textbox", {name: CHAT_PLACEHOLDER})).toHaveProperty("disabled", false);
});

it.each(["MiniMax M3.1 Flash (Free)", "GLM 5.3 Flash"])("displays the selected model's supplied label: %s", (label) => {
  render(<DraftConversation client={client} view={{...client.getSnapshot(), models: [{...MODEL, label}], model: MODEL.ref}} />);
  expect(screen.getByRole("combobox", {name: "Authoring model"}).textContent).toBe(label);
});

it("focuses New fleet ready for typing without clicking the composer", async () => {
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

it("focuses New fleet after leaving an existing chat without stealing focus on resume", async () => {
  await client.resume(DRAFT.id);
  render(<Live><Button onClick={() => client.fresh()}>New fleet</Button></Live>);
  const input = screen.getByRole("textbox", {name: CHAT_PLACEHOLDER});
  expect(document.activeElement).not.toBe(input);
  await userEvent.click(screen.getByRole("button", {name: "New fleet"}));
  expect(document.activeElement).toBe(input);
});

it("waits for the new composer to become ready before focusing", () => {
  const view = client.getSnapshot();
  const rendered = render(<DraftConversation client={client} view={{...view, busy: true}} />);
  const input = screen.getByRole("textbox", {name: CHAT_PLACEHOLDER});
  expect(document.activeElement).not.toBe(input);
  rendered.rerender(<DraftConversation client={client} view={view} />);
  expect(document.activeElement).toBe(input);
});
