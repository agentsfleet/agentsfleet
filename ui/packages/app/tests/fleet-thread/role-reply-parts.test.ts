import { ev, mockStream, renderThread, threadElement } from "./harness";
import { describe, expect, it } from "vitest";
import { fireEvent, screen, within } from "@testing-library/react";
import { OUTCOME } from "@/lib/events/event-summary";
import { LOADING_VERBS } from "@/components/layout/loading-verbs";

// The reply as assistant-ui message parts, rendered through the real runtime:
// the operator's turn splits into its trigger and a reply message whose status
// and parts drive every state below.

const COMPOSER_NAME = "Message this fleet…";
const TOOL_CALLS = "Tool calls";
const COPY_REPLY = "Copy reply";
const TOOL_STARTED_AT_MS = 1_000;
const TOOL_WALL_MS = 700;

function replyRow(): HTMLElement {
  const rows = document.querySelectorAll<HTMLElement>('[data-role="assistant"]');
  const row = rows[rows.length - 1];
  if (row === undefined) throw new Error("no reply row rendered");
  return row;
}

// The destructive icon beside a failed reply's own words.
function failureMark(words: string): SVGElement | null {
  const line = within(replyRow()).getByText(words).closest("[data-failed-outcome]");
  return line?.querySelector("svg.text-destructive") ?? null;
}

function messageRoot(of: HTMLElement): HTMLElement {
  const root = of.closest<HTMLElement>('[data-testid="fleet-message"]');
  if (root === null) throw new Error("row outside a message root");
  return root;
}

describe("FleetThread — reply parts", () => {
  it("test_inflight_turn_has_running_reply", () => {
    mockStream([ev({ id: "optim-1", role: "user", actor: "steer:pending", text: "deploy the canary", status: "optimistic", clientTimestamp: true })]);
    renderThread();
    expect(screen.getByText("deploy the canary")).toBeTruthy();
    // The reply row exists before any word, holding only the wait state.
    const reply = replyRow();
    expect(within(reply).getByRole("status", { name: "Queued" })).toBeTruthy();
    expect(within(reply).queryByRole("list", { name: TOOL_CALLS })).toBeNull();
    expect(within(reply).queryByRole("button")).toBeNull();
    // A running reply never marks the thread running: the composer still steers.
    const composer = screen.getByRole("textbox", { name: COMPOSER_NAME }) as HTMLTextAreaElement;
    expect(composer.disabled).toBe(false);
    fireEvent.change(composer, { target: { value: "and roll back" } });
    expect((screen.getByRole("button", { name: "Send" }) as HTMLButtonElement).disabled).toBe(false);
  });

  it("test_reply_renders_through_grouped_parts", () => {
    mockStream([ev({
      role: "user", actor: "operator", text: "Review it", status: "processed",
      reasoning: "Weighing the blast radius.", reply: "Opened the PR.",
      tools: [{ name: "search_repo", startedAtMs: TOOL_STARTED_AT_MS, ms: TOOL_WALL_MS, done: true }],
    })]);
    renderThread();
    const reply = replyRow();
    const chip = within(reply).getByRole("button", { name: /^Thought/ });
    const tools = within(reply).getByRole("list", { name: TOOL_CALLS });
    const answer = within(reply).getByText("Opened the PR.");
    expect(within(tools).getByText("search_repo")).toBeTruthy();
    // Reasoning, then tools, then the answer — the library's grouping order.
    expect(chip.compareDocumentPosition(tools) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(tools.compareDocumentPosition(answer) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("test_text_part_and_settled_row", () => {
    const streaming = ev({ id: "evt_stream", role: "user", actor: "operator", text: "Go", status: "received", reply: "Half an answer" });
    mockStream([streaming]);
    const view = renderThread();
    expect(within(replyRow()).getByLabelText("streaming")).toBeTruthy();
    expect(within(replyRow()).queryByRole("button", { name: COPY_REPLY })).toBeNull();
    expect(messageRoot(replyRow()).getAttribute("data-settled")).toBeNull();

    mockStream([{ ...streaming, status: "processed", reply: "The whole answer" }]);
    view.rerender(threadElement());
    expect(within(replyRow()).getByText("The whole answer")).toBeTruthy();
    expect(within(replyRow()).queryByLabelText("streaming")).toBeNull();
    expect(within(replyRow()).getByRole("button", { name: COPY_REPLY })).toBeTruthy();
    expect(messageRoot(replyRow()).getAttribute("data-settled")).toBe("true");
  });

  it("test_outcome_floor_without_text_part", () => {
    mockStream([ev({ role: "user", actor: "operator", text: "Anything?", status: "processed", outcome: OUTCOME.COMPLETED })]);
    const view = renderThread();
    expect(within(replyRow()).getByText(OUTCOME.COMPLETED)).toBeTruthy();
    expect(replyRow().getAttribute("data-failed")).toBeNull();
    expect(replyRow().querySelector("[data-failed-outcome]")).toBeNull();

    mockStream([ev({ role: "user", actor: "operator", text: "Anything?", status: "fleet_error", outcome: OUTCOME.FAILED })]);
    view.rerender(threadElement());
    expect(within(replyRow()).getByText(OUTCOME.FAILED)).toBeTruthy();
    expect(replyRow().getAttribute("data-failed")).toBe("true");
    // The failure carries its own mark, so it never reads as a short reply.
    expect(failureMark(OUTCOME.FAILED)).toBeTruthy();
  });

  it("should give an integration turn a reply row once it has only called tools", () => {
    mockStream([ev({
      id: "evt_hook", role: "system", actor: "webhook:github", text: "PR opened", status: "received",
      tools: [{ name: "read_file", startedAtMs: TOOL_STARTED_AT_MS, ms: null, done: false }],
    })]);
    renderThread();
    // The tick stays the trigger; the tool row belongs to the fleet's reply.
    expect(screen.getByText("PR opened")).toBeTruthy();
    expect(within(replyRow()).getByRole("list", { name: TOOL_CALLS })).toBeTruthy();
    expect(within(replyRow()).getByText("read_file")).toBeTruthy();
  });

  it("should render an errored reply's words as written, in the failed tone", () => {
    mockStream([ev({ role: "assistant", actor: "fleet", status: "fleet_error", reply: "**429** from the provider" })]);
    renderThread();
    // The dashboard's own sentence, not the model's markdown: no <strong>.
    expect(within(replyRow()).getByText("**429** from the provider")).toBeTruthy();
    expect(replyRow().querySelector("strong")).toBeNull();
    expect(replyRow().getAttribute("data-failed")).toBe("true");
    expect(failureMark("**429** from the provider")).toBeTruthy();
  });

  it("test_indicator_verb_and_accessible_name", () => {
    mockStream([ev({ id: "evt_working", role: "user", actor: "operator", text: "Run it", status: "received" })]);
    renderThread();
    const working = within(replyRow()).getByRole("status", { name: "Working" });
    expect(working.querySelector("[data-braille-spinner]")).toBeTruthy();
    const verb = LOADING_VERBS.find((candidate) => working.textContent?.includes(`${candidate}…`));
    expect(verb).toBeDefined();
  });
});
