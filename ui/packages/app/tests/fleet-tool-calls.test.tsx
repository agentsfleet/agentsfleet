import React from "react";
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import type { MessageState } from "@assistant-ui/react";
import { readTools, ToolCalls } from "../components/domain/FleetToolCalls";

afterEach(cleanup);

// readTools narrows an `unknown` metadata bag. The custom bag crosses the
// assistant-ui boundary untyped, so a malformed entry must be dropped — never
// crash the thread, never render a nameless tool.
function messageWith(tools: unknown): MessageState {
  return { metadata: { custom: { tools } } } as unknown as MessageState;
}

describe("readTools", () => {
  it("returns the tools array when every entry is well-formed", () => {
    const tools = [{ name: "grep", ms: 90, done: true }];
    expect(readTools(messageWith(tools))).toEqual(tools);
  });

  it("returns empty for a bag that carries no array", () => {
    expect(readTools(messageWith(undefined))).toEqual([]);
    expect(readTools(messageWith("not-an-array"))).toEqual([]);
    expect(readTools(messageWith({ name: "grep" }))).toEqual([]);
  });

  it("filters malformed entries instead of crashing the thread", () => {
    const mixed = [
      { name: "grep", ms: null, done: false },
      { name: 42, done: true },
      null,
      "tool",
      { done: true },
    ];
    expect(readTools(messageWith(mixed))).toEqual([{ name: "grep", ms: null, done: false }]);
  });
});

describe("ToolCalls", () => {
  it("renders nothing at all for an event with no tool calls", () => {
    const { container } = render(<ToolCalls tools={[]} />);
    expect(container.firstChild).toBeNull();
  });

  it("shows a running tool with its elapsed time, and a done tool with its final time", () => {
    render(
      <ToolCalls
        tools={[
          { name: "search_repo", ms: 1_400, done: false },
          { name: "read_file", ms: 200, done: true },
        ]}
      />,
    );
    const list = screen.getByRole("list", { name: "Tool calls" });
    expect(list).toBeTruthy();
    expect(screen.getByText("search_repo").closest("li")?.getAttribute("data-done")).toBeNull();
    expect(screen.getByText("read_file").closest("li")?.getAttribute("data-done")).toBe("true");
    expect(screen.getByText("1.4s")).toBeTruthy();
    expect(screen.getByText("200ms")).toBeTruthy();
  });

  // A just-started tool has no timing yet — the row renders without inventing one.
  it("renders a started tool with no time rather than a placeholder", () => {
    render(<ToolCalls tools={[{ name: "cordon", ms: null, done: false }]} />);
    expect(screen.getByText("cordon")).toBeTruthy();
    expect(screen.queryByText(/ms$|s$/)).toBeNull();
  });

  // The ms/s formatting boundary, both sides of it.
  it("formats the second boundary on both sides", () => {
    render(
      <ToolCalls
        tools={[
          { name: "fast", ms: 999, done: true },
          { name: "slow", ms: 1_000, done: true }, // pin test: literal is the contract
        ]}
      />,
    );
    expect(screen.getByText("999ms")).toBeTruthy();
    expect(screen.getByText("1.0s")).toBeTruthy();
  });
});
