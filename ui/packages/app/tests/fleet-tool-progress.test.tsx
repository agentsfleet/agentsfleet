import { ev, mockStream, renderThread, threadElement } from "./fleet-thread/harness";
import { expect, it } from "vitest";
import { fireEvent, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { FleetToolCall, FleetEventStatus } from "@/lib/streaming/fleet-stream-row";
import { TOOL_CALL_STATUS } from "@/lib/streaming/fleet-stream-tool-trace";
import { TOOL_NAME } from "@/components/domain/tool-call-shape";

const COMMAND = "nproc; cat /proc/loadavg";
const OUTPUT = "8\n0.08 0.04 0.00 1/199 6";
const {SUCCEEDED, FAILED, INTERRUPTED} = TOOL_CALL_STATUS;

function call(overrides: Partial<FleetToolCall> = {}): FleetToolCall {
  return {name: TOOL_NAME.SHELL, args: {command: COMMAND}, startedAtMs: Date.now(),
    done: true, status: SUCCEEDED, ms: 100, outputHead: OUTPUT, ...overrides};
}
function show(tools: FleetToolCall[], status: FleetEventStatus = "processed") {
  mockStream([ev({id: "progress-event", role: "user", actor: "operator", text: "Check performance", status, tools})]);
}
function openGroup(): void { fireEvent.click(screen.getByRole("button", {name: "Ran commands"})); }
const rows = () => within(screen.getByRole("list", {name: "Ran commands"})).getAllByRole("listitem");
const command = (row: HTMLElement) => within(row).getByRole("button", {name: /^(Ran|Running) command /});

it("groups calls behind one disclosure with keyboard-accessible individual command details", async () => {
  show([call(), call({name: TOOL_NAME.HTTP_REQUEST, status: FAILED}), call({name: "custom_check"})]);
  renderThread();
  const group = screen.getByRole("button", {name: "Ran commands"});
  expect(group.getAttribute("aria-expanded")).toBe("false");
  expect(screen.queryByRole("list", {name: "Ran commands"})).toBeNull();
  const user = userEvent.setup();
  group.focus(); await user.keyboard("{Enter}");
  const [shell, request, custom] = rows();
  expect(rows()).toHaveLength(3);
  expect(command(shell!).textContent).toContain("Ran commandshellSuccess");
  expect(command(request!).textContent).toContain("Ran commandhttp_requestFailed");
  expect(command(custom!).textContent).toContain("Ran commandcustom_checkSuccess");
  expect(screen.queryByText(/0.08/)).toBeNull();
  const first = command(shell!);
  first.focus(); await user.keyboard("{Enter}");
  expect(first.getAttribute("aria-expanded")).toBe("true");
  expect(shell!.textContent).toContain(COMMAND);
  expect(shell!.textContent).toContain("0.08 0.04 0.00 1/199 6");
  expect(shell!.querySelector('[data-command-outcome]')?.textContent).toBe("✓Success");
  expect(command(request!).getAttribute("aria-expanded")).toBe("false");
  await user.click(first);
  expect(shell!.textContent).not.toContain(COMMAND);
  await user.click(group);
  expect(screen.queryByRole("list", {name: "Ran commands"})).toBeNull();
});

it("keeps open details and row identity stable while another call streams and settles", async () => {
  const first = call({callId: "first"});
  const second = call({callId: "second", name: TOOL_NAME.HTTP_REQUEST, done: false, status: undefined, ms: null});
  show([first, second], "received");
  const view = renderThread();
  openGroup();
  const [finished, streaming] = rows();
  expect(streaming!.getAttribute("data-state")).toBe("running");
  expect(command(streaming!).textContent).toContain("Running command");
  expect(document.querySelectorAll('[data-tool-shimmer="true"]')).toHaveLength(1);
  await userEvent.setup().click(command(finished!));
  show([first, {...second, done: true, status: SUCCEEDED, ms: 200}, call({callId: "third"})]);
  view.rerender(threadElement());
  expect(rows()).toHaveLength(3);
  expect(rows()[0]).toBe(finished); expect(rows()[1]).toBe(streaming);
  expect(command(finished!).getAttribute("aria-expanded")).toBe("true");
  expect(command(streaming!).textContent).toContain("http_requestSuccess");
  expect(document.querySelectorAll('[data-tool-shimmer="true"]')).toHaveLength(0);
});

it("keeps repeated failures separate and reveals only the command the reader opens", async () => {
  const url = "https://api.fly.io/prometheus/example";
  show([call({name: TOOL_NAME.HTTP_REQUEST, args: {url}, status: FAILED, outputHead: "first refusal"}),
    call({name: TOOL_NAME.HTTP_REQUEST, args: {url}, status: FAILED, outputHead: "second refusal", exitCode: 9})]);
  renderThread(); openGroup();
  const [first, second] = rows();
  expect(screen.queryByText(/refusal/)).toBeNull();
  expect(screen.queryByText(new RegExp(url))).toBeNull();
  await userEvent.setup().click(command(second!));
  expect(second!.textContent).toContain("second refusal");
  expect(second!.querySelector('[data-command-outcome]')?.textContent).toBe("Failed(exit 9)");
  expect(first!.textContent).not.toContain("first refusal");
  expect(command(first!).getAttribute("aria-expanded")).toBe("false");
});

it.each([
  [call({status: INTERRUPTED}), "Interrupted"],
  [call({done: false, status: undefined, ms: null}), "Interrupted"],
  [call({status: undefined}), "Outcome unavailable"],
])("does not claim success for interrupted or unspecified outcomes %#", (tool, status) => {
  show([tool]); renderThread(); openGroup();
  fireEvent.click(command(rows()[0]!));
  expect(rows()[0]!.querySelector('[data-command-outcome]')?.textContent).toBe(status);
  expect(rows()[0]!.textContent).not.toContain("Success");
  expect(rows()[0]!.querySelector('[data-tool-shimmer="true"]')).toBeNull();
});

it("keeps generic command wording for unknown tools and shows recorded output literally", async () => {
  show([call({name: "custom_check", args: {internal_path: "/tmp/check.py"}, outputHead: "1309 tests passed; suite still running"})]);
  renderThread(); openGroup();
  expect(screen.getByText("custom_check")).toBeTruthy();
  expect(screen.queryByText(/1309|\/tmp\/check/)).toBeNull();
  await userEvent.setup().click(command(rows()[0]!));
  expect(screen.getByText(/1309 tests passed/)).toBeTruthy();
  expect(screen.queryByText(/suite completed|Steps completed/i)).toBeNull();
});

it("retains the individual calls inside a mixed command group", () => {
  show([call({name: TOOL_NAME.FILE_READ}), call({name: TOOL_NAME.MEMORY_RECALL}), ...Array.from({length: 6}, () => call())]);
  renderThread(); openGroup();
  expect(rows()).toHaveLength(8);
  expect(screen.getByText(TOOL_NAME.FILE_READ)).toBeTruthy();
  expect(screen.getByText(TOOL_NAME.MEMORY_RECALL)).toBeTruthy();
});
