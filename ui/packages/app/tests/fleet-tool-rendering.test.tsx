import { expect, it, vi } from "vitest";
import { screen } from "@testing-library/react";
import * as activity from "@/components/domain/tool-call-activity";
import type { FleetToolCall } from "@/lib/streaming/fleet-stream-row";
import { TOOL_CALL_STATUS } from "@/lib/streaming/fleet-stream-tool-trace";
import { ev, mockStream, renderThread, threadElement } from "./fleet-thread/harness";

it("keeps unchanged tool activity out of streamed text updates and refreshes changed outcomes", () => {
  const tool: FleetToolCall = {startedAtMs: Date.now(), name: "file_read", callId: "read-source", args: {path: "src/parse.ts"}, done: true,
    status: TOOL_CALL_STATUS.SUCCEEDED, ms: 10, outputHead: "source"};
  const summary = vi.spyOn(activity, "toolGroupActivity");
  const update = (reply: string, tools = [tool]) => mockStream([
    ev({id: "render-owner", role: "user", actor: "operator", text: "Investigate", status: "received", reply, tools}),
  ]);
  try {
    update("Starting");
    const view = renderThread();
    summary.mockClear();
    for (let index = 1; index <= 10; index++) {
      update(`Answer chunk ${index}`);
      view.rerender(threadElement());
    }
    console.info(`TOOL_GROUP_RECOMPUTATIONS text_updates=10 calls=${summary.mock.calls.length}`);
    expect(screen.getByText("Answer chunk 10")).toBeTruthy();
    expect(summary).toHaveBeenCalledTimes(0);
    update("Read failed", [{...tool, status: TOOL_CALL_STATUS.FAILED, outputHead: "read refused"}]);
    view.rerender(threadElement());
    expect(summary.mock.calls.length).toBeGreaterThan(0);
    expect(screen.getByRole("button", {name: /^Tool activity:/}).textContent).toContain("Failed");
  } finally {
    summary.mockRestore();
  }
});
