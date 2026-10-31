import { ev, mockStream, renderThread, threadElement } from "./harness";
import { expect, it } from "vitest";
import { fireEvent, screen, within } from "@testing-library/react";
import { CONTEXT_STATE } from "@/lib/context-progress";

const start = {generation: 1, state: CONTEXT_STATE.summarizing};
const end = {generation: 1, state: CONTEXT_STATE.resuming};
const TOOL_STARTED_AT_MS = 1_000;
function event() { return ev({id: "summarizing-run", actor: "operator", role: "user", text: "Continue the investigation", status: "received"}); }

it("announces summary progress once beside tools and a partial answer, then removes it on settlement", () => {
  const running = {...event(), reply: "Original partial answer", contextProgress: start,
    tools: [{name: "read_file", startedAtMs: TOOL_STARTED_AT_MS, ms: 10, done: true}]};
  mockStream([running]);
  const view = renderThread();
  expect(screen.getAllByText("Context summarizing…")).toHaveLength(1);
  expect(screen.getByText("Original partial answer")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", {name: "Ran commands"}));
  expect(screen.getByRole("list", {name: "Ran commands"})).toBeTruthy();
  mockStream([{...running, contextProgress: end}]);
  view.rerender(threadElement());
  expect(screen.getAllByText("Context summarized, resuming…")).toHaveLength(1);
  mockStream([{...running, status: "processed", reply: "Durable final answer"}]);
  view.rerender(threadElement());
  expect(screen.getByText("Durable final answer")).toBeTruthy();
  expect(screen.queryByText("Context summarizing…")).toBeNull();
  expect(screen.queryByText("Context summarized, resuming…")).toBeNull();
});

it("replaces the empty working indicator with one summary announcement", () => {
  mockStream([{...event(), contextProgress: start}]);
  renderThread();
  const rows = document.querySelectorAll<HTMLElement>('[data-role="assistant"]');
  const reply = rows.item(rows.length - 1);
  expect(within(reply).getAllByRole("status")).toHaveLength(1);
  expect(within(reply).getByRole("status").textContent).toContain("Context summarizing…");
  expect(within(reply).queryByRole("status", {name: "Working"})).toBeNull();
});

it("hides a recovered status hint and keeps the ordinary working indicator", () => {
  mockStream([{...event(), contextProgress: start, contextProgressHidden: true}]);
  renderThread();
  expect(screen.queryByText("Context summarizing…")).toBeNull();
  expect(screen.getByRole("status", {name: "Working"})).toBeTruthy();
});
