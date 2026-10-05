import { ev, mockStream, renderThread, threadElement } from "./fleet-thread/harness";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, screen, within } from "@testing-library/react";

import { CLIPPED_EDIT_NOTE, OUTPUT_CUT_LABEL, SHOW_ALL_LABEL } from "@/components/domain/FleetToolCallBody";
import { TOOL_BULLET, TOOL_CALLS_LABEL, TOOL_SUCCEEDED_GLYPH } from "@/components/domain/FleetToolCalls";
import { EMPTY_OUTPUT, OUTPUT_UNAVAILABLE } from "@/components/domain/tool-call-copy";
import { ARGS_NOT_RECORDED, TOOL_NAME } from "@/components/domain/tool-call-shape";
import type { LiveFrame, SavedToolCall } from "@/lib/api/events";
import { FRAME_KIND } from "@/lib/api/events-types";
import { applyLiveFrame } from "@/lib/streaming/fleet-stream-frames";
import { rowToEvent, type FleetEvent, type FleetEventStatus, type FleetToolCall } from "@/lib/streaming/fleet-stream-row";
import { TOOL_CALL_STATUS } from "@/lib/streaming/fleet-stream-tool-trace";
import { row } from "@/tests/helpers/fleet-stream-fixtures";

// Each call renders as Codex's cell: a status bullet, a verb and its target,
// dim figures, and what came back under a rail. Rendered through the thread,
// so the parts the cells read are the ones the converter builds.

const NOW = 10_000;
const RUNNING_FOR_MS = 2_000;
const TICK_MS = 1_000;
const DONE_MS = 700;
const { SUCCEEDED, FAILED, INTERRUPTED } = TOOL_CALL_STATUS;
const RECEIVED: FleetEventStatus = "received";
const PROCESSED: FleetEventStatus = "processed";
const CALL_ID = "f1:0";
const SUCCESS = "text-success";
const DESTRUCTIVE = "text-destructive";

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(NOW);
});

afterEach(() => {
  vi.useRealTimers();
});

function call(over: Partial<FleetToolCall> & { name: string }): FleetToolCall {
  return { startedAtMs: NOW - RUNNING_FOR_MS, ms: DONE_MS, done: true, status: SUCCEEDED, ...over };
}

function running(over: Partial<FleetToolCall> & { name: string }): FleetToolCall {
  return { startedAtMs: NOW - RUNNING_FOR_MS, ms: null, done: false, ...over };
}

function renderCalls(tools: FleetToolCall[], status: FleetEventStatus = PROCESSED, extra: Parameters<typeof ev>[0] | object = {}) {
  mockStream([ev({ id: "evt_tools", role: "user", actor: "operator", text: "Go", status, tools, ...extra })]);
  return renderThread();
}

const cells = () => [...document.querySelectorAll<HTMLElement>("li[data-tool]")];
const headerOf = (cell: Element) => cell.firstElementChild?.textContent ?? "";
const bulletOf = (cell: Element) => cell.querySelector("[data-tool-bullet]")!;

// Pin tests throughout: the literals are the words a cell draws.
const HEADERS: ReadonlyArray<readonly [FleetToolCall["name"], FleetToolCall["args"], string, string]> = [
  [TOOL_NAME.FILE_WRITE, { path: "/workspace/notes.md", content: "a\nb\n" }, "Writing notes.md (+2)", "Wrote notes.md (+2)"],
  [TOOL_NAME.FILE_APPEND, { path: "log.txt", content: "x" }, "Appending log.txt (+1)", "Appended log.txt (+1)"],
  [TOOL_NAME.FILE_DELETE, { path: "/workspace/old.md" }, "Deleting old.md", "Deleted old.md"],
  [TOOL_NAME.HTTP_REQUEST, { method: "POST", url: "https://api.example/run" }, "Requesting POST https://api.example/run", "Requested POST https://api.example/run"],
  [TOOL_NAME.HTTP_REQUEST, { url: "https://api.example/run" }, "Requesting GET https://api.example/run", "Requested GET https://api.example/run"],
  [TOOL_NAME.MEMORY_STORE, { key: "deploy" }, "Remembering deploy", "Remembered deploy"],
  [TOOL_NAME.MEMORY_FORGET, { key: "deploy" }, "Forgetting deploy", "Forgot deploy"],
];

describe("FleetThread — tool cells", () => {
  it("test_tool_row_reads_part_timing", () => {
    const working = ev({
      id: "evt_tools", role: "user", actor: "operator", text: "Look it up", status: RECEIVED,
      tools: [call({ name: "read_file", startedAtMs: NOW - 5_000 }), running({ name: "search_repo" })],
    });
    mockStream([working]);
    const view = renderThread();
    // Adjacent calls share one list.
    expect(screen.getAllByRole("list", { name: TOOL_CALLS_LABEL })).toHaveLength(1);
    const [done, live] = cells();
    expect(done?.getAttribute("data-done")).toBe("true");
    expect(headerOf(done!)).toContain(" · 0.7s");
    expect(live?.getAttribute("data-done")).toBeNull();
    expect(headerOf(live!)).toContain("2.0s");
    act(() => {
      vi.advanceTimersByTime(TICK_MS);
    });
    expect(headerOf(live!)).toContain("3.0s");
    // The ticking clock is hidden from assistive tech; a finished one is read.
    const clockOf = (cell: Element) => cell.querySelector(".tabular-nums")!;
    expect(clockOf(live!).getAttribute("aria-hidden")).toBe("true");
    expect(clockOf(done!).getAttribute("aria-hidden")).toBeNull();

    // The turn settles with the call never reported done: no clock claims it
    // runs, and the cell says it was cut off.
    mockStream([{ ...working, status: PROCESSED, reply: "Found it." }]);
    view.rerender(threadElement());
    const stranded = cells()[1]!;
    expect(headerOf(stranded)).toMatch(/^•Interrupted search_repo$/);
  });

  it("test_tool_cell_names_verb_and_target", () => {
    renderCalls(HEADERS.map(([name, args]) => running({ name, args })), RECEIVED);
    expect(cells().map(headerOf)).toEqual(HEADERS.map(([, , live]) => `${TOOL_BULLET}${live} · 2.0s`));
    cleanup();
    renderCalls(HEADERS.map(([name, args]) => call({ name, args })));
    expect(cells().map(headerOf)).toEqual(HEADERS.map(([, , , done]) => `${TOOL_SUCCEEDED_GLYPH}${done} · 0.7s`));
  });

  it("test_unknown_tool_cell_calls_by_name", () => {
    renderCalls([running({ name: "fly_status", args: { app: "x" } })], RECEIVED);
    expect(headerOf(cells()[0]!)).toContain('Calling fly_status({"app":"x"})');
    cleanup();
    renderCalls([call({ name: "fly_status", args: { app: "x" } }), call({ name: "fly_apps" })]);
    expect(headerOf(cells()[0]!)).toContain('Called fly_status({"app":"x"})');
    // No arguments: no parentheses, and no arguments disclosure.
    expect(headerOf(cells()[1]!)).toMatch(/^✓Called fly_apps · /);
    expect(within(cells()[1]!).queryByText("Details")).toBeNull();
    expect(within(cells()[0]!).getByText("Details")).toBeTruthy();
  });

  it("test_tool_cell_bullet_follows_status", () => {
    const failed = call({ name: TOOL_NAME.HTTP_REQUEST, args: { url: "https://x" }, status: FAILED, outputHead: "denied" });
    renderCalls([
      call({ name: TOOL_NAME.FILE_DELETE }), failed, call({ name: TOOL_NAME.FILE_DELETE, status: INTERRUPTED }),
      call({ name: TOOL_NAME.FILE_DELETE, status: undefined }), running({ name: TOOL_NAME.FILE_DELETE }),
    ], RECEIVED);
    const [ok, bad, cut, unsaid, live] = cells();
    expect(bulletOf(ok!).className).toContain(SUCCESS);
    expect(bulletOf(bad!).className).toContain(DESTRUCTIVE);
    // A failure says so in words, never by colour alone.
    expect(headerOf(bad!)).toContain("Requested GET https://x (failed)");
    expect(bulletOf(cut!).className).toContain(DESTRUCTIVE);
    expect(headerOf(cut!)).toMatch(/^•Interrupted /);
    // An older runner's call settled without saying how: no colour claims success.
    expect(bulletOf(unsaid!).className).not.toContain(SUCCESS);
    expect(bulletOf(unsaid!).className).toContain("text-text-dim");
    expect(bulletOf(live!).getAttribute("data-tool-shimmer")).toBe("true");
    expect(bulletOf(ok!).getAttribute("data-tool-shimmer")).toBe("false");
  });

  it("test_tool_cell_previews_three_rows", () => {
    const eight = Array.from({ length: 8 }, (_, index) => `line ${index + 1}`).join("\n");
    renderCalls([
      call({ name: TOOL_NAME.HTTP_REQUEST, callId: CALL_ID, outputHead: eight, outputLineCount: 8 }),
      call({ name: TOOL_NAME.FILE_DELETE, outputHead: "one\ntwo\nthree", outputLineCount: 4 }),
    ]);
    const [long, short] = cells();
    expect(long!.textContent).toContain("└line 1");
    expect(long!.textContent).toContain("line 3");
    expect(long!.textContent).not.toContain("line 4");
    expect(within(long!).getByText("+5 lines")).toBeTruthy();
    expect(within(long!).getByRole("button", { name: SHOW_ALL_LABEL })).toBeTruthy();
    // One line left out says so in the singular; with no call id there is
    // nothing to read it by, so no "show all".
    expect(within(short!).getByText("+1 line")).toBeTruthy();
    expect(within(short!).queryByRole("button", { name: SHOW_ALL_LABEL })).toBeNull();
  });

  it("test_tool_cell_names_empty_and_unknown_output", () => {
    renderCalls([
      call({ name: TOOL_NAME.FILE_DELETE, outputHead: "", outputLineCount: 0 }),
      call({ name: TOOL_NAME.FILE_DELETE, status: undefined }),
      call({ name: TOOL_NAME.FILE_DELETE, status: INTERRUPTED }),
    ]);
    const [empty, unsaid, cut] = cells();
    expect(empty!.textContent).toContain(`└${EMPTY_OUTPUT}`);
    expect(unsaid!.textContent).toContain(`└${OUTPUT_UNAVAILABLE}`);
    expect(cut!.textContent).toContain(`└${OUTPUT_UNAVAILABLE}`);
    // A running call shows no body yet.
    cleanup();
    renderCalls([running({ name: TOOL_NAME.FILE_DELETE })], RECEIVED);
    expect(cells()[0]!.textContent).not.toContain("└");
  });

  it("test_command_cell_renders_like_codex", () => {
    const four = "echo one\necho two\necho three\necho four";
    renderCalls([
      call({ name: TOOL_NAME.EXEC_COMMAND, args: { cmd: four }, status: FAILED, exitCode: 2, outputHead: "boom", outputLineCount: 1 }),
      call({ name: TOOL_NAME.SHELL, args: { command: "ls" }, exitCode: 0, outputHead: "a.md", outputLineCount: 1 }),
    ]);
    const [ran, shell] = cells();
    expect(headerOf(ran!)).toMatch(/^•Ran echo one \(exit 2\) · /);
    // The exit code says how it failed; "(failed)" would repeat it.
    expect(headerOf(ran!)).not.toContain("(failed)");
    expect(within(ran!).getByText("(exit 2)", { exact: false }).className).toContain(DESTRUCTIVE);
    expect(ran!.textContent).toContain("│echo two│echo three│… +1 line└boom");
    expect(headerOf(shell!)).toMatch(/^✓Ran ls · /);
    expect(headerOf(shell!)).not.toContain("exit");
  });

  it("test_edit_cell_renders_diff", () => {
    renderCalls([
      call({ name: TOOL_NAME.FILE_EDIT, args: { path: "/workspace/deploy.yaml", old_text: "a\nb", new_text: "a\nc\nd" } }),
      call({ name: TOOL_NAME.FILE_EDIT_HASHED, args: { path: "big.md", old_text: "x".repeat(300), new_text: "y" }, callId: CALL_ID }),
    ]);
    const [edit, clipped] = cells();
    expect(headerOf(edit!)).toMatch(/^✓Edited deploy\.yaml \(\+2 −1\) · /);
    const rows = [...edit!.querySelectorAll("[data-diff]")];
    expect(rows.map((row) => [row.getAttribute("data-diff"), row.textContent])).toEqual([
      ["context", " a"], ["removed", "-removed: b"], ["added", "+added: c"], ["added", "+added: d"],
    ]);
    expect(rows[1]!.className).toContain("bg-destructive/10");
    expect(rows[2]!.className).toContain("bg-success/10");
    // A succeeded edit shows its diff alone.
    expect(edit!.textContent).not.toContain(EMPTY_OUTPUT);
    // Arguments the runner may have cut short draw no diff and no counts.
    expect(headerOf(clipped!)).toMatch(/^✓Edited big\.md · /);
    expect(clipped!.querySelector("[data-diff]")).toBeNull();
    expect(within(clipped!).getByText(CLIPPED_EDIT_NOTE, { exact: false })).toBeTruthy();
    expect(within(clipped!).getByRole("button", { name: SHOW_ALL_LABEL })).toBeTruthy();
  });

  it("should open a call in full from show all, and close it again", () => {
    // The read itself is the dialog's to test; here it never answers.
    vi.stubGlobal("fetch", vi.fn(() => new Promise<Response>(() => {})));
    try {
      const view = renderCalls([call({ name: TOOL_NAME.HTTP_REQUEST, args: { url: "https://x" }, callId: CALL_ID, outputHead: "1\n2\n3\n4", outputLineCount: 4 })]);
      fireEvent.click(screen.getByRole("button", { name: SHOW_ALL_LABEL }));
      const dialog = screen.getByRole("dialog", { name: "Requested GET https://x" });
      expect(dialog).toBeTruthy();
      fireEvent.keyDown(dialog, { key: "Escape" });
      expect(screen.queryByRole("dialog")).toBeNull();
      // Opened on one call, it closes rather than read another the saved trace puts in its row.
      fireEvent.click(screen.getByRole("button", { name: SHOW_ALL_LABEL }));
      mockStream([ev({ id: "evt_tools", role: "user", actor: "operator", text: "Go", status: PROCESSED, tools: [call({ name: TOOL_NAME.HTTP_REQUEST, args: { url: "https://y" }, callId: "f1:9", outputHead: "1\n2\n3\n4", outputLineCount: 4 })] })]);
      view.rerender(threadElement());
      expect(screen.queryByRole("dialog")).toBeNull();
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it("test_reload_shows_saved_tool_rows", () => {
    // One turn's calls, as the runner saved them at settle.
    const saved: SavedToolCall[] = [
      { call_id: "f1:0", name: TOOL_NAME.FILE_READ, arguments: { path: "/workspace/a.md" }, status: SUCCEEDED, output_head: "# A", output_line_count: 1, duration_ms: DONE_MS },
      { call_id: "f1:1", name: TOOL_NAME.HTTP_REQUEST, arguments: { method: "POST", url: "https://x" }, status: FAILED, output_head: "denied", output_line_count: 1, duration_ms: DONE_MS },
      { call_id: "f1:2", name: TOOL_NAME.FILE_EDIT, arguments: { path: "d.yaml", old_text: "a\nb", new_text: "a\nc" }, status: SUCCEEDED, duration_ms: DONE_MS },
    ];
    const settled = { event_id: "evt_reload", actor: "steer:user_abc", status: PROCESSED, response_text: "Done." };
    // Live: the same calls frame by frame through the reducer, then the turn settles.
    let live: FleetEvent[] = [{ ...rowToEvent(row({ ...settled, status: RECEIVED })), role: "user" }];
    for (const savedCall of saved) {
      const { call_id, name, arguments: args, duration_ms, ...outcome } = savedCall;
      const frames: LiveFrame[] = [
        { kind: FRAME_KIND.TOOL_CALL_STARTED, event_id: settled.event_id, name, call_id, args_redacted: args },
        { kind: FRAME_KIND.TOOL_CALL_COMPLETED, event_id: settled.event_id, name, call_id, ms: duration_ms, ...outcome },
      ];
      for (const frame of frames) live = applyLiveFrame(live, frame, NOW);
    }
    live = live.map((event) => ({ ...event, status: PROCESSED }));
    // Reloaded: the saved row the server reads, mapped the way the page maps it.
    const reloaded: FleetEvent = { ...rowToEvent(row({ ...settled, tool_calls: { calls: saved, omitted_call_count: 0 } })), role: "user" };

    const toolRegion = () => [...document.querySelectorAll("[data-explored], li[data-tool]")].map((cell) => cell.textContent);
    mockStream(live);
    renderThread();
    const liveCells = toolRegion();
    cleanup();
    mockStream([reloaded]);
    renderThread();
    // Pin test: the reloaded turn reads exactly as it did live.
    expect(toolRegion()).toEqual(liveCells);
    expect(liveCells).toHaveLength(3);
    expect(liveCells[1]).toContain("Requested POST https://x (failed)");
  });

  it("test_cells_claim_only_what_they_hold", () => {
    const long = call({ name: TOOL_NAME.HTTP_REQUEST, args: { url: "https://x" }, callId: CALL_ID, outputHead: "1\n2\n3\n4", outputLineCount: 4 });
    renderCalls([long], RECEIVED);
    // The call finished but its turn runs on: its full record is not posted yet.
    expect(within(cells()[0]!).getByText("+1 line")).toBeTruthy();
    expect(within(cells()[0]!).queryByRole("button", { name: SHOW_ALL_LABEL })).toBeNull();
    cleanup();
    renderCalls([long]);
    expect(within(cells()[0]!).getByRole("button", { name: SHOW_ALL_LABEL })).toBeTruthy();
    cleanup();
    // Output the runner kept only the edges of says it continues, with show all.
    renderCalls([call({ name: TOOL_NAME.HTTP_REQUEST, args: { url: "https://x" }, callId: CALL_ID, outputHead: "{\"items\":[", outputTail: "]}", outputLineCount: 1 })]);
    const [cell] = cells();
    expect(cell!.textContent).toContain(OUTPUT_CUT_LABEL);
    expect(within(cell!).getByRole("button", { name: SHOW_ALL_LABEL })).toBeTruthy();
    cleanup();
    // Arguments the runner dropped are never invented.
    renderCalls([call({ name: TOOL_NAME.HTTP_REQUEST }), call({ name: TOOL_NAME.FILE_WRITE })]);
    const [request, write] = cells();
    expect(headerOf(request!)).toContain(`Requested ${ARGS_NOT_RECORDED}`);
    expect(headerOf(request!)).not.toContain("GET");
    expect(headerOf(write!)).toContain(`Wrote ${ARGS_NOT_RECORDED}`);
    cleanup();
    // Text the runner may have cut at its leaf cap ends in a mark.
    const cutUrl = `https://x/${"a".repeat(260)}`;
    renderCalls([call({ name: TOOL_NAME.HTTP_REQUEST, args: { url: cutUrl } })]);
    expect(headerOf(cells()[0]!)).toContain(`${cutUrl}…`);
  });

  it("test_failed_edit_shows_its_error", () => {
    renderCalls([call({ name: TOOL_NAME.FILE_EDIT, status: FAILED, args: { path: "d.yaml", old_text: "a", new_text: "b" }, outputHead: "old_text not found", outputLineCount: 1 })]);
    const [edit] = cells();
    // What it tried stays visible; the error says it did not apply.
    expect(edit!.querySelectorAll("[data-diff]")).toHaveLength(2);
    expect(edit!.textContent).toContain("└old_text not found");
    expect(headerOf(edit!)).toContain("(failed)");
  });

  it("test_tool_output_is_literal_text", () => {
    const hostile = "**bold** <img src=x onerror=alert(1)>";
    renderCalls([call({ name: TOOL_NAME.HTTP_REQUEST, args: { url: "https://x" }, outputHead: hostile, outputLineCount: 1 })]);
    const [cell] = cells();
    expect(cell!.textContent).toContain(hostile);
    expect(cell!.querySelector("img, strong")).toBeNull();
  });

  it("test_cell_text_is_readable", () => {
    renderCalls([call({ name: TOOL_NAME.FILE_DELETE, args: { path: "a.md" }, outputHead: "ok", outputLineCount: 1 })]);
    const [cell] = cells();
    // text-dim measures 4.41:1 on the light page; readable text uses text-subtle.
    expect(cell!.className).toContain("text-text-subtle");
    expect(cell!.className).not.toContain("text-text-dim");
  });

  it("test_omitted_calls_render_count", () => {
    renderCalls([call({ name: TOOL_NAME.FILE_DELETE })], PROCESSED, { omittedCallCount: 4 });
    expect(screen.getByText("4 more calls not recorded")).toBeTruthy();
    cleanup();
    renderCalls([call({ name: TOOL_NAME.FILE_DELETE })], PROCESSED, { omittedCallCount: 1 });
    expect(screen.getByText("1 more call not recorded")).toBeTruthy();
    cleanup();
    renderCalls([call({ name: TOOL_NAME.FILE_DELETE })]);
    expect(screen.queryByText(/more calls? not recorded/)).toBeNull();
  });
});
