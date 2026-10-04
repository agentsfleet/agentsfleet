import { ev, mockStream, renderThread } from "@/tests/fleet-thread/harness";
import { describe, expect, it } from "vitest";
import { render, screen, within } from "@testing-library/react";
import type { MessageState } from "@assistant-ui/react";

import { EXPLORED_LABEL, EXPLORING_LABEL, FleetExplored } from "./FleetExplored";
import { TOOL_BULLET, TOOL_CALLS_LABEL } from "./FleetToolCalls";
import { FAILED_MARK, TOOL_NAME } from "./tool-call-copy";
import type { FleetEventStatus, FleetToolCall } from "@/lib/streaming/fleet-stream-row";
import { TOOL_CALL_STATUS } from "@/lib/streaming/fleet-stream-tool-trace";

// Reads fold under one Explored cell, through the reply's group map: the
// group is the library's, built from the same parts the cells read.

const STARTED_AT_MS = 1_000;
const DONE_MS = 300;
const { SUCCEEDED, FAILED } = TOOL_CALL_STATUS;
const RECEIVED: FleetEventStatus = "received";

function read(path: string, over: Partial<FleetToolCall> = {}): FleetToolCall {
  return { name: TOOL_NAME.FILE_READ, args: { path }, startedAtMs: STARTED_AT_MS, ms: DONE_MS, done: true, status: SUCCEEDED, ...over };
}

function renderReply(tools: FleetToolCall[], status: FleetEventStatus = "processed") {
  mockStream([ev({ id: "evt_reads", role: "user", actor: "operator", text: "Look", status, tools })]);
  renderThread();
}

const groups = () => [...document.querySelectorAll<HTMLElement>("[data-explored]")];
// A line as it reads on screen: its rail glyph only where one shows, then its words.
const linesOf = (group: HTMLElement) => within(group).getAllByRole("listitem").map((line) => {
  const [glyph, words] = [...(line.firstElementChild?.children ?? [])];
  return `${glyph?.classList.contains("invisible") ? " " : glyph?.textContent}${words?.textContent}`;
});

describe("FleetExplored", () => {
  it("should draw no line for a part that is no tool call", () => {
    const content = [{ type: "text", text: "x" }, { type: "tool-call", toolName: TOOL_NAME.FILE_READ, args: { path: "a.md" } }] as unknown as MessageState["content"];
    render(<FleetExplored content={content} indices={[0, 1, 5]} running={false} />);
    expect(screen.getAllByRole("listitem").map((line) => line.textContent?.slice(1))).toEqual(["Read a.md"]);
  });
});

describe("FleetThread — Explored", () => {
  it("test_consecutive_reads_fold_under_explored", () => {
    renderReply([
      read("/workspace/a.md"),
      read("docs/b.md", { name: TOOL_NAME.FILE_READ_HASHED }),
      read("/workspace/a.md"),
      { name: TOOL_NAME.MEMORY_RECALL, args: { query: "deploy window" }, startedAtMs: STARTED_AT_MS, ms: DONE_MS, done: true, status: SUCCEEDED },
      { name: TOOL_NAME.MEMORY_LIST, args: { category: "runbooks" }, startedAtMs: STARTED_AT_MS, ms: DONE_MS, done: true, status: SUCCEEDED },
    ]);
    const [group] = groups();
    expect(groups()).toHaveLength(1);
    expect(within(group!).getByText(EXPLORED_LABEL)).toBeTruthy();
    // Pin test: the literals are the lines Codex's fold draws.
    expect(linesOf(group!)).toEqual(["└Read a.md, b.md", " Search deploy window in memory", " List memory runbooks"]);
    // Folded calls draw no cells of their own.
    expect(screen.queryByRole("list", { name: TOOL_CALLS_LABEL })).toBeNull();
  });

  it("test_non_read_call_splits_explored", () => {
    renderReply([
      read("a.md"),
      { name: TOOL_NAME.FILE_WRITE, args: { path: "b.md", content: "x" }, startedAtMs: STARTED_AT_MS, ms: DONE_MS, done: true, status: SUCCEEDED },
      read("c.md"),
    ]);
    // Read, write, read: Explored, Wrote, Explored, in that order.
    const [first, second] = groups();
    const tools = screen.getByRole("list", { name: TOOL_CALLS_LABEL });
    expect(groups()).toHaveLength(2);
    expect(linesOf(first!)).toEqual(["└Read a.md"]);
    expect(within(tools).getByText("Wrote")).toBeTruthy();
    expect(linesOf(second!)).toEqual(["└Read c.md"]);
    expect(first!.compareDocumentPosition(tools) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(tools.compareDocumentPosition(second!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("test_explored_header_tracks_running", () => {
    renderReply([read("a.md"), { name: TOOL_NAME.FILE_READ, args: { path: "b.md" }, startedAtMs: STARTED_AT_MS, ms: null, done: false }], RECEIVED);
    const [group] = groups();
    expect(group!.getAttribute("data-explored")).toBe(EXPLORING_LABEL);
    expect(within(group!).getByText(TOOL_BULLET).getAttribute("data-tool-shimmer")).toBe("true");
    // A read still running already names what it reads.
    expect(linesOf(group!)).toEqual(["└Read a.md, b.md"]);
  });

  it("test_explored_header_settles", () => {
    renderReply([read("a.md")]);
    const [group] = groups();
    expect(group!.getAttribute("data-explored")).toBe(EXPLORED_LABEL);
    const bullet = within(group!).getByText(TOOL_BULLET);
    expect(bullet.getAttribute("data-tool-shimmer")).toBe("false");
    expect(bullet.className).toContain("text-text-dim");
  });

  it("test_failed_read_marks_its_explored_line", () => {
    renderReply([read("a.md"), read("b.md", { status: FAILED }), read("c.md"), read("d.md", { status: TOOL_CALL_STATUS.INTERRUPTED })]);
    const [group] = groups();
    // The failed read keeps its own line, marked in words; the reads after it
    // start a new line rather than joining a failure.
    expect(linesOf(group!)).toEqual(["└Read a.md", ` Read b.md ${FAILED_MARK}`, " Read c.md", ` Read d.md ${FAILED_MARK}`]);
    expect(within(group!).getAllByText(FAILED_MARK, { exact: false })[0]!.className).toContain("text-destructive");
    expect(group!.querySelectorAll("[data-failed]")).toHaveLength(2);
  });
});
