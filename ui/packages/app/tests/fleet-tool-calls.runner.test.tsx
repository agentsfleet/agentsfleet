import { ev, mockStream, renderThread } from "./fleet-thread/harness";
import { describe, expect, it } from "vitest";
import { cleanup, screen, within } from "@testing-library/react";

import { TOOL_CALLS_LABEL } from "@/components/domain/FleetToolCalls";
import { EXPLORED_LABEL } from "@/components/domain/FleetExplored";
import { TOOL_NAME } from "@/components/domain/tool-call-shape";
import type { FleetToolCall } from "@/lib/streaming/fleet-stream-row";
import { TOOL_CALL_STATUS } from "@/lib/streaming/fleet-stream-tool-trace";

// The runner's tools beyond files, requests and memory, drawn in the thread:
// a plan as its checklist, a patch as its diff, a hashed edit by its line, and
// the web and schedule lookups folded with the other looks.

const STARTED_AT_MS = 1_000;
const DONE_MS = 400;

function call(name: string, args: FleetToolCall["args"]): FleetToolCall {
  return { name, args, startedAtMs: STARTED_AT_MS, ms: DONE_MS, done: true, status: TOOL_CALL_STATUS.SUCCEEDED };
}

function renderCalls(tools: FleetToolCall[]) {
  mockStream([ev({ id: "evt_runner", role: "user", actor: "operator", text: "Go", status: "processed", tools })]);
  renderThread();
}

const cell = (name: string) => document.querySelector<HTMLElement>(`li[data-tool="${name}"]`)!;

describe("FleetThread — runner tool cells", () => {
  it("test_runner_cells_render_plan_patch_and_line", () => {
    renderCalls([call(TOOL_NAME.UPDATE_PLAN, {
      explanation: "One step left",
      plan: [{ step: "Read the logs", status: "completed" }, { step: "Open a PR", status: "in_progress" }, { step: "Tell the team", status: "pending" }],
    })]);
    const plan = cell(TOOL_NAME.UPDATE_PLAN);
    expect(plan.textContent).toContain("Updated plan");
    expect(plan.textContent).toContain("└One step left");
    expect([...plan.querySelectorAll("[data-plan-step]")].map((step) => step.textContent)).toEqual(["✔", "◐", "□"]);
    // Pin test: the literals are what a screen reader hears for each state.
    expect(plan.textContent).toContain("Read the logs (done)");
    expect(plan.textContent).toContain("Open a PR (in progress)");
    expect(plan.textContent).toContain("Tell the team (to do)");
    cleanup();

    // With no explanation, the first step takes the rail.
    renderCalls([call(TOOL_NAME.UPDATE_PLAN, { plan: [{ step: "Only step", status: "pending" }] })]);
    expect(cell(TOOL_NAME.UPDATE_PLAN).textContent).toContain("└□ Only step");
    cleanup();

    renderCalls([call(TOOL_NAME.APPLY_PATCH, { patch: "*** Update File: /workspace/a.ts\n-old\n+new\n+more" })]);
    const patch = cell(TOOL_NAME.APPLY_PATCH);
    expect(patch.firstElementChild?.textContent).toMatch(/^✓Edited a\.ts \(\+2 −1\) · /);
    expect([...patch.querySelectorAll("[data-diff]")].map((row) => row.getAttribute("data-diff"))).toEqual(["removed", "added", "added"]);
    cleanup();

    // A hashed edit that names its line by tag carries no old text to diff.
    renderCalls([call(TOOL_NAME.FILE_EDIT_HASHED, { path: "/workspace/a.md", target: "L10:abc", new_text: "b" })]);
    const edit = cell(TOOL_NAME.FILE_EDIT_HASHED);
    expect(edit.firstElementChild?.textContent).toMatch(/^✓Edited a\.md at L10:abc · /);
    expect(edit.querySelector("[data-diff]")).toBeNull();
    cleanup();

    // The web and schedule lookups change nothing, so they fold with the other looks.
    renderCalls([
      call(TOOL_NAME.WEB_SEARCH, { query: "dragonfly cluster" }),
      call(TOOL_NAME.WEB_FETCH, { url: "https://docs.example/ops" }),
      call(TOOL_NAME.CRON_LIST, undefined),
      call(TOOL_NAME.PUSHOVER, { message: "Done" }),
    ]);
    const group = document.querySelector<HTMLElement>("[data-explored]")!;
    expect(within(group).getByText(EXPLORED_LABEL)).toBeTruthy();
    expect(group.textContent).toContain("Search dragonfly cluster on the web");
    expect(group.textContent).toContain("Fetch https://docs.example/ops");
    expect(group.textContent).toContain("List schedules");
    // A notification changes something, so it stays a cell of its own.
    const tools = screen.getByRole("list", { name: TOOL_CALLS_LABEL });
    expect(within(tools).getByText("Notified")).toBeTruthy();
  });
});
