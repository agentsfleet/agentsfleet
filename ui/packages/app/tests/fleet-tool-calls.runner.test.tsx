import { ev, mockStream, renderThread } from "./fleet-thread/harness";
import { describe, expect, it } from "vitest";
import { cleanup, fireEvent, screen, within } from "@testing-library/react";

import { TOOL_CALLS_LABEL } from "@/components/domain/activity/tools/FleetToolCalls";
import { TOOL_NAME } from "@/components/domain/activity/tools/presentation/tool-call-shape";
import type { FleetToolCall } from "@/lib/streaming/fleet-stream-row";
import { TOOL_CALL_STATUS } from "@/lib/streaming/fleet-stream-tool-trace";

// The runner's tools beyond files, requests and memory, drawn in the thread:
// a plan as its checklist, a patch as the files and counts the runner
// recorded, a hashed edit by its line, and the web and schedule lookups folded
// with the other looks.

const STARTED_AT_MS = 1_000;
const DONE_MS = 400;

function call(name: string, args: FleetToolCall["args"]): FleetToolCall {
  return { name, args, startedAtMs: STARTED_AT_MS, ms: DONE_MS, done: true, status: TOOL_CALL_STATUS.SUCCEEDED };
}

function renderCalls(tools: FleetToolCall[]) {
  mockStream([ev({ id: "evt_runner", role: "user", actor: "operator", text: "Go", status: "processed", tools })]);
  renderThread();
  fireEvent.click(screen.getByRole("button", {name: /^Tool activity:/}));
  for (const heading of document.querySelectorAll("[data-command-heading]")) {
    const button = heading.closest("button");
    if (button) fireEvent.click(button);
  }
}

const cell = (name: string) => document.querySelector<HTMLElement>(`li[data-tool="${name}"]`)!;

describe("FleetThread — runner tool cells", () => {
  it("test_runner_cells_render_plan_patch_and_line", () => {
    renderCalls([call(TOOL_NAME.UPDATE_PLAN, {
      explanation: "One step left",
      plan: [{ step: "Read the logs", status: "completed" }, { step: "Open a PR", status: "in_progress" }, { step: "Tell the team", status: "pending" }],
    })]);
    const plan = cell(TOOL_NAME.UPDATE_PLAN);
    expect(plan.textContent).toContain(TOOL_NAME.UPDATE_PLAN);
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

    const patched = { patch: "*** Update File: /workspace/a.ts\n-old\n+new\n+more" };
    renderCalls([{ ...call(TOOL_NAME.APPLY_PATCH, patched), changes: [{ path: "a.ts", added: 24, removed: 8 }] }]);
    const patch = cell(TOOL_NAME.APPLY_PATCH);
    expect(patch.firstElementChild?.textContent).toContain("Edited files");
    expect([...patch.querySelectorAll("[data-changed-file]")].map((file) => file.textContent)).toEqual(["a.ts"]);
    expect(patch.textContent).toContain("a.ts +24 −8");
    expect(patch.querySelector("[data-diff]"), "no line is worked out of the patch text").toBeNull();
    cleanup();

    // Each file the patch touched gets its own line; only the first takes the rail.
    const twoFiles = [{ path: "a.ts", added: 24, removed: 8 }, { path: "b.ts", added: 3, removed: 1 }];
    renderCalls([{ ...call(TOOL_NAME.APPLY_PATCH, patched), changes: twoFiles }]);
    const both = cell(TOOL_NAME.APPLY_PATCH);
    expect([...both.querySelectorAll("[data-changed-file]")].map((file) => file.textContent)).toEqual(["a.ts", "b.ts"]);
    expect(both.textContent).toContain("b.ts +3 −1");
    const glyphs = [...both.querySelectorAll("[data-changed-file]")].map((file) => file.parentElement?.parentElement?.firstElementChild);
    expect(glyphs.map((glyph) => glyph?.classList.contains("invisible"))).toEqual([false, true]);
    cleanup();

    // A patch the runner recorded no change for claims none.
    renderCalls([call(TOOL_NAME.APPLY_PATCH, patched)]);
    expect(cell(TOOL_NAME.APPLY_PATCH).querySelector("[data-changed-file]")).toBeNull();
    expect(cell(TOOL_NAME.APPLY_PATCH).textContent).not.toContain("+2");
    cleanup();

    // A hashed edit that names its line by tag carries no old text to diff.
    renderCalls([call(TOOL_NAME.FILE_EDIT_HASHED, { path: "/workspace/a.md", target: "L10:abc", new_text: "b" })]);
    const edit = cell(TOOL_NAME.FILE_EDIT_HASHED);
    expect(edit.querySelector("[data-command-heading]")?.textContent).toBe("Edited file Success");
    expect(edit.textContent).toContain('"path":"/workspace/a.md"');
    expect(edit.querySelector("[data-diff]")).toBeNull();
    cleanup();

    // The web and schedule lookups change nothing, so they fold with the other looks.
    renderCalls([
      call(TOOL_NAME.WEB_SEARCH, { query: "nats cluster" }),
      call(TOOL_NAME.WEB_FETCH, { url: "https://docs.example/ops" }),
      call(TOOL_NAME.CRON_LIST, undefined),
      call(TOOL_NAME.NOTIFY, { message: "Done" }),
    ]);
    const tools = screen.getByRole("list", {name: TOOL_CALLS_LABEL});
    expect(within(tools).getAllByRole("listitem")).toHaveLength(4);
    expect(cell(TOOL_NAME.WEB_SEARCH).textContent).toContain("nats cluster");
    expect(cell(TOOL_NAME.WEB_FETCH).textContent).toContain("https://docs.example/ops");
    expect(cell(TOOL_NAME.CRON_LIST)).toBeTruthy();
    expect(cell(TOOL_NAME.NOTIFY).textContent).toContain("Done");
  });
});
