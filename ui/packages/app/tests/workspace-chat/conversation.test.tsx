import { useSyncExternalStore } from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { DraftClient } from "@/lib/api/drafts";
import type { DraftView } from "@/lib/api/draft-state";
import { DraftConversation } from "@/components/domain/chat/DraftConversation";
import { SetupProgress } from "@/components/domain/chat/SetupProgress";
import { LOAD_MORE_COPY } from "@/components/domain/chat/DraftLoadMore";
import { ACTIVITY_KIND } from "@/lib/events/fleet-activity";
import { ChatFixture, CHAT_PROPS } from "./fixture";
import { DRAFT, FLEET, MESSAGE, MODEL, response } from "@/tests/helpers/draft-fixtures";

let fixture: ChatFixture;
let client: DraftClient;
beforeEach(async () => {
  fixture = new ChatFixture(); fixture.mount();
  client = new DraftClient({...CHAT_PROPS, subject: "creator"});
  await client.initialize();
});
afterEach(() => {cleanup(); client.dispose(); vi.unstubAllGlobals();});
function LiveConversation() {
  const view = useSyncExternalStore(client.subscribe, client.getSnapshot, client.getSnapshot);
  return <DraftConversation client={client} view={view} />;
}

it("folds a long request without losing its full text", async () => {
  const text = `Investigate the development environment. ${"Include concrete evidence. ".repeat(60)}End of the full request.`;
  render(<DraftConversation client={client} view={{...client.getSnapshot(), messages: [{...MESSAGE, text}]}} />);
  expect(screen.queryByText(/End of the full request/)).toBeNull();
  fireEvent.click(screen.getByRole("button", {name: "Show full message"}));
  expect(screen.getByText(/End of the full request/)).toBeTruthy();
  fireEvent.click(screen.getByRole("button", {name: "Show less"}));
  expect(screen.queryByText(/End of the full request/)).toBeNull();
});

it.each([ACTIVITY_KIND.setupSaved, ACTIVITY_KIND.setupReady])("keeps the saved occurrence and readiness before the next working turn (%s)", kind => {
  const activity = {event_id: "setup:one", event_type: "activity", status: "recorded",
    request_json: JSON.stringify({kind, revision: "1"}), created_at: "1970-01-01T00:00:01Z"};
  const view = {...client.getSnapshot(), fleet: FLEET, activities: [activity], messages: [
    MESSAGE, {...MESSAGE, sequence: "2", text: "Change the setup", state: "running" as const, created_at: "1970-01-01T00:00:02Z"},
  ]};
  render(<DraftConversation client={client} view={view}><p>Configuration readiness</p></DraftConversation>);
  const saved = screen.getByText(kind === ACTIVITY_KIND.setupSaved ? "Save done v1" : "Setup done · v1");
  const checks = screen.getByText("Configuration readiness");
  const working = screen.getByRole("status", {name: "Working"});
  expect(saved.compareDocumentPosition(checks) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(checks.compareDocumentPosition(working) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
});

it("keeps saved setup actions inside the conversation history", () => {
  render(<DraftConversation client={client} view={{...client.getSnapshot(), messages: [MESSAGE]}}>
    <SetupProgress fleet={FLEET} definition={DRAFT.candidate?.definition} workspace={CHAT_PROPS.workspace} busy={false}
      onContinue={() => {}} onRetrySchedule={() => {}} />
  </DraftConversation>);
  const saved = screen.getByRole("heading", {name: "Save done v1"});
  expect(screen.getByRole("log").contains(saved)).toBe(true);
  expect(screen.queryByRole("link", {name: "Continue setup in chat"})).toBeNull();
});

it.each([false, true])("shows the new-fleet entrance only before an empty draft is saved (%s)", (saved) => {
  render(<DraftConversation client={client} view={{...client.getSnapshot(), messages: [], fleet: saved ? FLEET : null}}>
    {saved ? <SetupProgress fleet={FLEET} definition={DRAFT.candidate?.definition} workspace={CHAT_PROPS.workspace} busy={false}
      onContinue={() => {}} onRetrySchedule={() => {}} /> : null}
  </DraftConversation>);
  const entrance = screen.queryByRole("heading", {name: "What should your fleet do?"});
  const suggestion = screen.queryByRole("button", {name: /Review pull requests/});
  if (saved) {
    expect(entrance).toBeNull();
    expect(suggestion).toBeNull();
    expect(screen.getByRole("log").contains(screen.getByRole("heading", {name: "Save done v1"}))).toBe(true);
  } else {
    expect(entrance).toBeTruthy();
    expect(suggestion).toBeTruthy();
    expect(screen.queryByRole("link", {name: "Continue setup in chat"})).toBeNull();
  }
});

it("stops authoring at the durable watermark and replaces Working with the terminal result", async () => {
  fixture.messages = [{...MESSAGE, role: "assistant", text: undefined, state: "running"}];
  const base = fixture.fetch.getMockImplementation()!;
  fixture.fetch.mockImplementation(async (input, init) => {
    if ((typeof input === "string" ? input : input instanceof URL ? input.href : input.url).endsWith("/cancellations")) {
      fixture.messages = [{...MESSAGE, role: "assistant", state: "cancelled", text: undefined}];
      return response({cancelled_through: MESSAGE.sequence, stopped: [{sequence: MESSAGE.sequence, state: "cancelled"}]});
    }
    return base(input, init);
  });
  await client.resume(DRAFT.id); render(<LiveConversation />);
  expect(screen.getByRole("status", {name: "Working"})).toBeTruthy();
  fireEvent.click(screen.getByRole("button", {name: "Stop authoring"}));
  expect(await screen.findByText("Request stopped.")).toBeTruthy();
  await waitFor(() => expect(screen.queryByRole("button", {name: "Stop authoring"})).toBeNull());
  expect(screen.queryByRole("status", {name: "Working"})).toBeNull();
});

it.each([
  ["insufficient_balance", "There aren’t enough credits to finish this request."],
  ["model_unavailable", "This model isn’t available right now."],
  ["context_too_large", "This conversation is too long for the selected model."],
  ["provider_failed", "The model didn’t finish its response."],
  ["provider_usage_unknown", "Usage is still being reconciled."],
  ["attempt_timed_out", "The model didn’t finish in time, so there’s no complete response."],
  ["policy_changed", "The setup or its permissions changed during this request."],
  ["new-failure-reason", "The model didn’t finish its response."],
])("explains the recorded failure without asking for another message (%s)", (reason, copy) => {
  const view: DraftView = {...client.getSnapshot(), truncated: true, messages: [{...MESSAGE, role: "assistant", state: "failed",
    text: undefined, failure: {reason, error_code: "UZ-TEST-001", recovery_action: "retry_message"}}]};
  render(<DraftConversation client={client} view={view} />);
  expect(screen.getByRole("log").textContent).toContain(copy);
  expect(screen.getByRole("log").textContent).not.toContain("UZ-TEST-001");
  fireEvent.click(screen.getByRole("button", {name: "View details"}));
  expect(screen.getByRole("log").textContent).toContain("UZ-TEST-001");
  expect(screen.getByRole("log").textContent).not.toContain("Send a new message to try again");
  expect(screen.queryByRole("button", {name: /Retry request/})).toBeNull();
  expect(screen.getByText(/Showing the latest conversation messages/)).toBeTruthy();
  expect(screen.queryByRole("button", {name: "Stop authoring"})).toBeNull();
});

it("loads more eligible authoring models and removes the control when the list ends", async () => {
  const base = fixture.fetch.getMockImplementation()!;
  fixture.fetch.mockImplementation(async (input, init) => {
    const path = (typeof input === "string" ? input : input instanceof URL ? input.href : input.url);
    if (path.includes("/models?")) return response(path.includes("starting_after=")
      ? {items: [{...MODEL, ref: "model:second", label: "Second model"}], next_cursor: null}
      : {items: [MODEL], next_cursor: "next"});
    return base(input, init);
  });
  await client.initialize(); render(<LiveConversation />);
  fireEvent.click(screen.getByRole("button", {name: "Load more models"}));
  await waitFor(() => expect(screen.queryByRole("button", {name: "Load more models"})).toBeNull());
  expect(client.getSnapshot().models.map((model) => model.label)).toEqual([MODEL.label, "Second model"]);
  expect(client.getSnapshot().model).toBe(MODEL.ref);
});

it("does not offer more models after losing access, even when an older view had a cursor", () => {
  render(<DraftConversation client={client} view={{...client.getSnapshot(), moreModels: true, inaccessible: true}} />);
  expect(screen.queryByRole("button", {name: "Load more models"})).toBeNull();
  expect(screen.getByRole("button", {name: "Send message"})).toHaveProperty("disabled", true);
});

it("keeps the selected model available when loading the next page fails", async () => {
  const next = Promise.withResolvers<Response>();
  const base = fixture.fetch.getMockImplementation()!;
  fixture.fetch.mockImplementation(async (input, init) => {
    const path = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    if (path.includes("/models?")) return path.includes("starting_after=")
      ? next.promise : response({items: [MODEL], next_cursor: "next"});
    return base(input, init);
  });
  await client.initialize(); render(<LiveConversation />);
  fireEvent.click(screen.getByRole("button", {name: LOAD_MORE_COPY.models.label}));
  expect(screen.getByRole("button", {name: "Loading…"})).toHaveProperty("disabled", true);
  expect(screen.getByRole("combobox", {name: "Authoring model"}).textContent).toContain(MODEL.label);
  await act(async () => next.resolve(response({}, 503)));
  expect(screen.getByRole("alert").textContent).toContain(LOAD_MORE_COPY.models.failure);
  expect(screen.getByRole("combobox", {name: "Authoring model"}).textContent).toContain(MODEL.label);
  expect(screen.getByRole("button", {name: LOAD_MORE_COPY.models.label})).toHaveProperty("disabled", false);
  expect(client.getSnapshot().models).toEqual([MODEL]);
});

it.each([false, true])("selects an authoring model with the keyboard and handles a rejected selection (%s)", async (rejectSelection) => {
  const second = {...MODEL, ref: "model:second", label: "Second model"};
  const base = fixture.fetch.getMockImplementation()!;
  fixture.fetch.mockImplementation(async (input, init) => {
    const path = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    return path.includes("/models?") ? response({items: [MODEL, second], next_cursor: null}) : base(input, init);
  });
  await client.initialize();
  const select = vi.spyOn(client, "selectModel");
  if (rejectSelection) select.mockRejectedValueOnce(new Error("selection interrupted"));
  render(<LiveConversation />);
  const user = userEvent.setup();
  await user.click(screen.getByRole("combobox", {name: "Authoring model"}));
  await user.keyboard("{End}{Enter}");
  await waitFor(() => expect(select).toHaveBeenCalledWith(second.ref));
  await waitFor(() => expect(client.getSnapshot().model).toBe(rejectSelection ? MODEL.ref : second.ref));
  expect(screen.queryByRole("listbox")).toBeNull();
});
