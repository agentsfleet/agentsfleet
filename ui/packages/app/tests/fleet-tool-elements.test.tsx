import type { FleetToolCall } from "@/lib/streaming/fleet-stream-row";
import { expect, it } from "vitest";
import { fireEvent, screen, within } from "@testing-library/react";
import { TOOL_CALL_STATUS } from "@/lib/streaming/fleet-stream-tool-trace";
import { ev, mockStream, renderThread } from "./fleet-thread/harness";

it("keeps commands, reads, edits and a new tool readable inside one reply", () => {
  const {container} = renderTools([
    {name: "exec_command", args: {cmd: "printf ready\nprintf checked"}, status: TOOL_CALL_STATUS.FAILED, outputHead: "permission denied", exitCode: 2},
    {name: "file_read", args: {path: "/workspace/app.ts"}, outputHead: "export const ready = true;"},
    {name: "file_edit", args: {path: "/workspace/app.ts", old_text: "ready = false", new_text: "ready = true"}},
    {name: "custom_release_report", args: {release: "秋"}, outputHead: "Release report recorded"},
  ]);
  const command = screen.getByLabelText("Command result");
  expect(command.querySelector("code")?.textContent).toBe("printf ready\nprintf checked");
  expect(screen.getByText("permission denied")).toBeTruthy();
  expect(screen.getByText("export const ready = true;")).toBeTruthy();
  expect(container.querySelector('[data-diff="removed"]')?.textContent).toContain("ready = false");
  expect(container.querySelector('[data-diff="added"]')?.textContent).toContain("ready = true");
  expect(screen.getByText("Release report recorded")).toBeTruthy();
  expect(container.textContent).not.toMatch(/not supported|unsupported/i);
  const failed = container.querySelector('[data-tool="exec_command"]');
  expect(failed?.getAttribute("data-state")).toBe("failed");
  expect(failed?.textContent).toContain("(exit 2)");
});

it.each(["constructor", "toString", "hasOwnProperty", "__proto__"])("uses the generic tool view for %s", name => {
  const {container} = renderTools([{name, args: {message: "recorded input"}, status: TOOL_CALL_STATUS.INTERRUPTED, outputHead: "run interrupted"}]);
  const cell = container.querySelector('[data-tool]');
  expect(cell?.getAttribute("data-state")).toBe("interrupted");
  expect(cell?.textContent).toContain(name);
  expect(screen.getByText("Input")).toBeTruthy();
  expect(screen.getByText("run interrupted")).toBeTruthy();
  expect(cell?.textContent).not.toContain("Success");
});

it("renders command and output markup as code and text without executing it", () => {
  const {container} = renderTools([{name: "shell", args: {command: "<script>do-not-run</script>"}, outputHead: "<img src=x onerror=do-not-run>"}]);
  expect(screen.getByLabelText("Command result").textContent).toContain("<script>do-not-run</script>");
  expect(screen.getByText("<img src=x onerror=do-not-run>")).toBeTruthy();
  expect(container.querySelector("script, img")).toBeNull();
});

it("does not claim success when the runner returns no outcome status", () => {
  const {container} = renderTools([{name: "file_edit", args: {path: "a.ts", old_text: "old", new_text: "new"}, status: undefined}]);
  const cell = container.querySelector('[data-tool="file_edit"]');
  expect(cell?.getAttribute("data-state")).toBe("settled");
  expect(cell?.textContent).not.toContain("Success");
  expect(cell?.querySelector('[data-diff="added"]')?.textContent).toContain("new");
});

function renderTools(tools: (Partial<FleetToolCall> & {name: string})[]) {
  mockStream([ev({id: "outcomes", role: "user", actor: "operator", text: "Review release", reply: "Recorded results.",
    tools: tools.map((tool, index) => ({startedAtMs: 1000, ms: 10, done: true, status: TOOL_CALL_STATUS.SUCCEEDED, callId: `call-${index}`, ...tool})),
  })]);
  const view = renderThread();
  fireEvent.click(screen.getByRole("button", {name: /^Tool activity:/}));
  const list = screen.getByRole("list", {name: "Tool activity"});
  for (const button of within(list).getAllByRole("button", {expanded: false})) fireEvent.click(button);
  return view;
}
