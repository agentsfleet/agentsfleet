import { useSyncExternalStore } from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { DraftClient } from "@/lib/api/drafts";
import type { DraftView } from "@/lib/api/draft-state";
import { DraftConversation } from "@/components/domain/chat/DraftConversation";
import { SetupProgress } from "@/components/domain/chat/SetupProgress";
import { LOAD_MORE_COPY } from "@/components/domain/chat/DraftLoadMore";
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

it("keeps saved setup actions inside the conversation history", () => {
  render(<DraftConversation client={client} view={{...client.getSnapshot(), messages: [MESSAGE]}}>
    <SetupProgress fleet={FLEET} definition={DRAFT.candidate?.definition} workspace={CHAT_PROPS.workspace} busy={false}
      onContinue={() => {}} onRetrySchedule={() => {}} />
  </DraftConversation>);
  const open = screen.getByRole("link", {name: "Open fleet"});
  expect(screen.getByRole("log").contains(open)).toBe(true);
  expect(open.getAttribute("href")).toBe(`/w/${CHAT_PROPS.workspace}/fleets/${FLEET.id}`);
  expect(screen.getByText(/When you send it a message/)).toBeTruthy();
});

it.each([false, true])("shows the new-fleet entrance only before an empty draft is saved (%s)", (saved) => {
  render(<DraftConversation client={client} view={{...client.getSnapshot(), messages: [], fleet: saved ? FLEET : null}}>
    {saved ? <SetupProgress fleet={FLEET} definition={DRAFT.candidate?.definition} workspace={CHAT_PROPS.workspace} busy={false}
      onContinue={() => {}} onRetrySchedule={() => {}} /> : null}
  </DraftConversation>);
  const entrance = screen.queryByRole("heading", {name: "What should your fleet do?"});
  const suggestion = screen.queryByRole("button", {name: /Fix a build/});
  if (saved) {
    expect(entrance).toBeNull();
    expect(suggestion).toBeNull();
    expect(screen.getByRole("log").contains(screen.getByRole("link", {name: "Open fleet"}))).toBe(true);
  } else {
    expect(entrance).toBeTruthy();
    expect(suggestion).toBeTruthy();
    expect(screen.queryByRole("link", {name: "Open fleet"})).toBeNull();
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
  expect(await screen.findByText("Authoring stopped.")).toBeTruthy();
  await waitFor(() => expect(screen.queryByRole("button", {name: "Stop authoring"})).toBeNull());
  expect(screen.queryByRole("status", {name: "Working"})).toBeNull();
});

it.each([
  ["insufficient_balance", "The account balance could not cover this setup turn."],
  ["model_unavailable", "The selected setup model is no longer available."],
  ["context_too_large", "The conversation and instructions exceed the model’s context limit."],
  ["provider_failed", "The setup turn failed after model usage was recorded. No detailed cause was recorded."],
  ["provider_usage_unknown", "The model request ended without confirmed usage."],
  ["attempt_timed_out", "The setup turn exceeded its time limit."],
  ["policy_changed", "The setup or its access policy changed during this turn."],
  ["new-failure-reason", "The setup turn failed. No detailed cause was recorded."],
])("explains the recorded failure without asking for another message (%s)", (reason, copy) => {
  const view: DraftView = {...client.getSnapshot(), truncated: true, messages: [{...MESSAGE, role: "assistant", state: "failed",
    text: undefined, failure: {reason, error_code: "UZ-TEST-001", recovery_action: "retry_message"}}]};
  render(<DraftConversation client={client} view={view} />);
  expect(screen.getByRole("log").textContent).toContain(copy);
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
