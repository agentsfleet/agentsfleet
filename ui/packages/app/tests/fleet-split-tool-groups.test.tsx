import { afterEach, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { AssistantRuntimeProvider, ThreadPrimitive, useExternalStoreRuntime } from "@assistant-ui/react";

import type { FleetEvent, FleetToolCall } from "@/lib/streaming/fleet-stream-row";
import { TOOL_CALL_STATUS } from "@/lib/streaming/fleet-stream-tool-trace";
import { formatCompactCount, formatRunDuration } from "@/lib/events/run-figures-format";
import { formatDollars } from "@/lib/money";
import { renderFleetMessage } from "@/components/domain/fleetMessageRenderers";
import { toReplyMessage } from "@/components/domain/fleetReplyMessage";
import { convertEvent } from "@/components/domain/useFleetEventStream";

// The event stream orders every tool before the answer. Native parts can still
// arrive as [tool, text, tool], so the order is set on the converted message.
const WALL_MS = 65_000;
const TOKENS = 12_400;
const COST_NANOS = "2500000";
const RUN_DETAILS = "Run details";
const GROUP = /^Tool activity:/;

afterEach(cleanup);

const call = (id: string): FleetToolCall => ({startedAtMs: 1_000, name: "file_read", callId: id, args: {path: `src/${id}.ts`}, done: true,
  status: TOOL_CALL_STATUS.SUCCEEDED, ms: 10, outputHead: id});

function Reply({tools, after, status = "processed"}: {tools: readonly FleetToolCall[]; after: number; status?: FleetEvent["status"]}) {
  const event: FleetEvent = {id: "split-run", role: "assistant", actor: "fleet", text: "", reply: "Between the calls.", outcome: "", failureLabel: null,
    failureDetail: null, createdAt: new Date(0), status, tools: [...tools], wallMs: WALL_MS, tokens: TOKENS, costNanos: COST_NANOS};
  const runtime = useExternalStoreRuntime<FleetEvent>({
    isRunning: false,
    messages: [event],
    convertMessage: row => {
      const message = toReplyMessage(convertEvent(row), row);
      const parts = Array.isArray(message.content) ? message.content : [];
      const text = parts.filter(part => part.type === "text"), calls = parts.filter(part => part.type === "tool-call");
      return {...message, content: [...calls.slice(0, after), ...text, ...calls.slice(after)]};
    },
    onNew: async () => {},
  });
  return <AssistantRuntimeProvider runtime={runtime}><ThreadPrimitive.Messages>{renderFleetMessage}</ThreadPrimitive.Messages></AssistantRuntimeProvider>;
}

const groups = () => screen.getAllByRole("button", {name: GROUP});
const details = () => screen.queryAllByRole("button", {name: RUN_DETAILS});

it("keeps exactly one reachable run-details entry, with the whole run's figures, across a split reply", () => {
  render(<Reply tools={[call("a"), call("b"), call("c")]} after={2} />);
  const [first, last] = groups() as [HTMLElement, HTMLElement];
  expect(groups()).toHaveLength(2);
  // Neither partial group claims the whole run's duration.
  expect(first.getAttribute("aria-label")).toBe("Tool activity: Worked · 2 tools");
  expect(last.getAttribute("aria-label")).toBe("Tool activity: Read file src/c.ts");
  fireEvent.click(first);
  expect(details()).toHaveLength(0);
  fireEvent.click(last);
  expect(details()).toHaveLength(1);
  fireEvent.click(details()[0]!);
  const popover = screen.getByRole("dialog", {name: RUN_DETAILS});
  expect(within(popover).getByText(formatRunDuration(WALL_MS))).toBeTruthy();
  expect(within(popover).getByText(formatCompactCount(TOKENS))).toBeTruthy();
  expect(within(popover).getByText(formatDollars(COST_NANOS))).toBeTruthy();
});

it("moves the entry to the newest group without remounting an open one", () => {
  const view = render(<Reply tools={[call("a"), call("b")]} after={2} />);
  const [only] = groups() as [HTMLElement];
  expect(only.getAttribute("aria-label")).toBe(`Tool activity: Worked for ${formatRunDuration(WALL_MS)} · 2 tools`);
  fireEvent.click(only);
  expect(details()).toHaveLength(1);

  view.rerender(<Reply tools={[call("a"), call("b"), call("c")]} after={2} />);
  expect(groups()).toHaveLength(2);
  expect(groups()[0]).toBe(only);
  expect(only.getAttribute("aria-expanded")).toBe("true");
  expect(only.getAttribute("aria-label")).toBe("Tool activity: Worked · 2 tools");
  expect(details()).toHaveLength(0);
  fireEvent.click(groups()[1]!);
  expect(details()).toHaveLength(1);
});

it("holds the entry back while the run is still going", () => {
  render(<Reply tools={[call("a"), call("b")]} after={1} status="received" />);
  for (const group of groups()) fireEvent.click(group);
  expect(details()).toHaveLength(0);
});
