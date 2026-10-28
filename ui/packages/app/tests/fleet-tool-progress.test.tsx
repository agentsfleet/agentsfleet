import { ev, mockStream, renderThread, threadElement } from "./fleet-thread/harness";
import { expect, it } from "vitest";
import { screen, within } from "@testing-library/react";
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
const rows = () => within(screen.getByRole("list", {name: "Tool calls"})).getAllByRole("listitem");

it("should show each round trip separately with its recorded name and closed input and output", async () => {
  show([call(), call({name: TOOL_NAME.HTTP_REQUEST, status: FAILED}), call({name: "custom_check"})]);
  renderThread();
  const [shell, request, custom] = rows();
  expect(rows()).toHaveLength(3);
  expect(shell!.textContent).toContain("shellDone");
  expect(request!.textContent).toContain("http_requestFailed");
  expect(custom!.textContent).toContain("custom_checkDone");
  expect(screen.queryByText(/Steps completed|Run a command|Contact a service/)).toBeNull();
  expect(screen.queryByText(new RegExp(COMMAND))).toBeNull();
  expect(screen.queryByText(/0.08/)).toBeNull();
  const user = userEvent.setup();
  const input = within(shell!).getByRole("button", {name: "Input"});
  input.focus(); await user.keyboard("{Enter}");
  expect(input.getAttribute("aria-expanded")).toBe("true");
  expect(shell!.textContent).toContain(COMMAND);
  expect(within(request!).getByRole("button", {name: "Input"}).getAttribute("aria-expanded")).toBe("false");
  await user.click(within(shell!).getByRole("button", {name: "Output"}));
  expect(shell!.textContent).toContain("0.08 0.04 0.00 1/199 6");
  await user.click(input);
  expect(shell!.textContent).not.toContain(COMMAND);
});

it("should keep existing entries and open input stable while another call streams and settles", async () => {
  const first = call({callId: "first"});
  const second = call({callId: "second", name: TOOL_NAME.HTTP_REQUEST, done: false, status: undefined, ms: null});
  show([first, second], "received");
  const view = renderThread();
  const [finished, streaming] = rows();
  expect(streaming!.getAttribute("data-state")).toBe("running");
  expect(within(streaming!).getByRole("button", {name: "Output"})).toHaveProperty("disabled", true);
  expect(document.querySelectorAll('[data-tool-shimmer="true"]')).toHaveLength(1);
  await userEvent.setup().click(within(finished!).getByRole("button", {name: "Input"}));
  show([first, {...second, done: true, status: SUCCEEDED, ms: 200}, call({callId: "third"})]);
  view.rerender(threadElement());
  expect(rows()).toHaveLength(3);
  expect(rows()[0]).toBe(finished); expect(rows()[1]).toBe(streaming);
  expect(within(finished!).getByRole("button", {name: "Input"}).getAttribute("aria-expanded")).toBe("true");
  expect(streaming!.textContent).toContain("http_requestDone");
  expect(document.querySelectorAll('[data-tool-shimmer="true"]')).toHaveLength(0);
});

it("should keep errors separate for repeated calls without exposing URLs until input is opened", async () => {
  const url = "https://api.fly.io/prometheus/example";
  show([call({name: TOOL_NAME.HTTP_REQUEST, args: {url}, status: FAILED, outputHead: "first refusal"}),
    call({name: TOOL_NAME.HTTP_REQUEST, args: {url}, status: FAILED, outputHead: "second refusal"})]);
  renderThread();
  const [first, second] = rows();
  expect(screen.queryByText(/refusal/)).toBeNull();
  expect(screen.queryByText(new RegExp(url))).toBeNull();
  await userEvent.setup().click(within(second!).getByRole("button", {name: "Error"}));
  expect(second!.textContent).toContain("second refusal");
  expect(first!.textContent).not.toContain("first refusal");
  expect(within(first!).getByRole("button", {name: "Error"}).getAttribute("aria-expanded")).toBe("false");
});

it.each([
  [call({status: INTERRUPTED}), "Interrupted"],
  [call({done: false, status: undefined, ms: null}), "Interrupted"],
  [call({status: undefined}), "Outcome unavailable"],
])("should not claim success for interrupted or unspecified outcomes %#", (tool, status) => {
  show([tool]); renderThread();
  expect(rows()[0]!.textContent).toContain(status);
  expect(rows()[0]!.textContent).not.toContain("Done");
  expect(rows()[0]!.querySelector('[data-tool-shimmer="true"]')).toBeNull();
});

it("should preserve unknown tool names and show stdout without inventing business descriptions", async () => {
  show([call({name: "custom_check", args: {internal_path: "/tmp/check.py"}, outputHead: "1309 tests passed; suite still running"})]);
  renderThread();
  expect(screen.getByText("custom_check")).toBeTruthy();
  expect(screen.queryByText(/1309|\/tmp\/check/)).toBeNull();
  await userEvent.setup().click(screen.getByRole("button", {name: "Output"}));
  expect(screen.getByText(/1309 tests passed/)).toBeTruthy();
  expect(screen.queryByText(/suite completed|Steps completed/i)).toBeNull();
});

it("should keep read calls separate rather than folding them into an exploration summary", () => {
  show([call({name: TOOL_NAME.FILE_READ}), call({name: TOOL_NAME.MEMORY_RECALL}), ...Array.from({length: 6}, () => call())]);
  renderThread();
  expect(rows()).toHaveLength(8);
  expect(screen.getByText(TOOL_NAME.FILE_READ)).toBeTruthy();
  expect(screen.getByText(TOOL_NAME.MEMORY_RECALL)).toBeTruthy();
  expect(document.querySelector("[data-explored]")).toBeNull();
});
