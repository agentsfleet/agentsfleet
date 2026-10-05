import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

import { CLIPPED_EDIT_NOTE } from "./FleetToolCallBody";
import { FleetScopeProvider, useFleetScope } from "./FleetScope";
import {
  ARGS_NOT_KEPT_NOTE,
  FleetToolOutputDialog,
  LOADING_LABEL,
  MAX_SHOWN_ROWS,
  NOT_KEPT_NOTE,
  OUTPUT_CUT_NOTE,
  READ_FAILED_NOTE,
  keptRows,
} from "./FleetToolOutputDialog";
import { TOOL_NAME } from "./tool-call-shape";
import type { ToolResult } from "./fleetReplyMessage";
import { TOOL_CALL_STATUS } from "@/lib/streaming/fleet-stream-tool-trace";

const SCOPE = { workspaceId: "ws_1", fleetId: "flt_1" };
const EVENT_ID = "evt_1";
const CALL_ID = "f1:3";
const VERB = "Requested";
const TARGET = "GET https://api.example/run";
const TITLE = `${VERB} ${TARGET}`;
const HTTP_NOT_FOUND = 404;
const HTTP_ERROR = 500;
const SAVED: ToolResult = {
  status: TOOL_CALL_STATUS.SUCCEEDED,
  outputHead: "line 1\nline 2\n",
  outputTail: "line 223\nline 224\n",
  outputLineCount: 224,
  callId: CALL_ID,
};

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function stubRead(status: number, body: unknown = {}) {
  const fetchMock = vi.fn(async () => new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } }));
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}

function renderDialog(over: { name?: string; outcome?: ToolResult; onClose?: () => void } = {}) {
  return render(
    <FleetToolOutputDialog
      scope={SCOPE}
      eventId={EVENT_ID}
      callId={CALL_ID}
      name={over.name ?? TOOL_NAME.HTTP_REQUEST}
      verb={VERB}
      target={TARGET}
      outcome={over.outcome ?? SAVED}
      onClose={over.onClose ?? (() => {})}
    />,
  );
}

const numbers = () => [...document.querySelectorAll("[data-line-number]")].map((cell) => cell.getAttribute("data-line-number"));

describe("FleetToolOutputDialog", () => {
  it("test_output_dialog_shows_full_output", async () => {
    const output = Array.from({ length: 224 }, (_, index) => `line ${index + 1}`).join("\n");
    const fetchMock = stubRead(200, { call_id: CALL_ID, arguments: { url: "https://api.example/run" }, truncated_arguments: false, output, output_line_count: 224, truncated: false });
    renderDialog();
    expect(screen.getByRole("dialog", { name: TITLE })).toBeTruthy();
    expect(screen.getByText(LOADING_LABEL)).toBeTruthy();
    await waitFor(() => expect(numbers()).toHaveLength(224));
    expect(numbers().at(-1)).toBe("224");
    expect(screen.getByText("line 224")).toBeTruthy();
    // The id is encoded once, through the same-origin proxy.
    expect(fetchMock).toHaveBeenCalledWith(
      "/live/v1/workspaces/ws_1/fleets/flt_1/events/evt_1/tool-calls/f1%3A3",
      expect.objectContaining({ redirect: "manual" }),
    );
    expect(screen.getByRole("button", { name: "Copy output" })).toBeTruthy();
    expect(screen.getByText("Details")).toBeTruthy();
    expect(screen.queryByText(OUTPUT_CUT_NOTE)).toBeNull();
  });

  it("should draw a whole edit's diff and say when the output was cut", async () => {
    const before = `${"x".repeat(300)}\nkeep`;
    stubRead(200, { call_id: CALL_ID, arguments: { path: "big.md", old_text: before, new_text: "keep\nnew" }, output: "ok", truncated: true });
    renderDialog({ name: TOOL_NAME.FILE_EDIT });
    await waitFor(() => expect(screen.getByText(OUTPUT_CUT_NOTE)).toBeTruthy());
    const rows = [...document.querySelectorAll("[data-diff]")].map((row) => row.getAttribute("data-diff"));
    expect(rows).toEqual(["removed", "context", "added"]);
    expect(screen.queryByText(CLIPPED_EDIT_NOTE)).toBeNull();
  });

  it("should show no arguments when the full read carries none", async () => {
    stubRead(200, { arguments: [1], output: "done" });
    renderDialog();
    await waitFor(() => expect(screen.getByText("done")).toBeTruthy());
    expect(screen.queryByText("Details")).toBeNull();
  });

  it("test_output_dialog_falls_back_when_not_kept", async () => {
    stubRead(HTTP_NOT_FOUND);
    renderDialog();
    await waitFor(() => expect(screen.getByText(NOT_KEPT_NOTE)).toBeTruthy());
    // Head from line 1, the gap marked, the tail where it falls.
    expect(numbers()).toEqual(["1", "2", "223", "224"]);
    expect(screen.getByText("⋯")).toBeTruthy();
    expect(screen.getByText("line 223")).toBeTruthy();
    // What the thread kept is not the whole output, so nothing offers to copy it.
    expect(screen.queryByRole("button", { name: "Copy output" })).toBeNull();
  });

  it("should fall back the same way when the read fails, and say it failed", async () => {
    stubRead(HTTP_ERROR);
    renderDialog();
    await waitFor(() => expect(screen.getByText(READ_FAILED_NOTE)).toBeTruthy());
    expect(numbers()).toEqual(["1", "2", "223", "224"]);
  });

  it("test_output_dialog_aborts_its_read_on_close", () => {
    let sent: AbortSignal | undefined;
    vi.stubGlobal("fetch", vi.fn((_url: string, init: RequestInit) => {
      sent = init.signal ?? undefined;
      return new Promise<Response>(() => {});
    }));
    const onClose = vi.fn();
    const view = renderDialog({ onClose });
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(sent?.aborted).toBe(false);
    view.unmount();
    // A closed dialog leaves no read running out its timeout.
    expect(sent?.aborted).toBe(true);
  });

  it("should ignore a read that answers after the dialog closed", async () => {
    let answer: (response: Response) => void = () => {};
    vi.stubGlobal("fetch", vi.fn(() => new Promise<Response>((settle) => { answer = settle; })));
    const view = renderDialog();
    view.unmount();
    answer(new Response(JSON.stringify({ output: "late" }), { status: 200 }));
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(screen.queryByText("late")).toBeNull();
  });

  it("test_output_dialog_notes_dropped_arguments", async () => {
    stubRead(200, { call_id: CALL_ID, arguments: {}, truncated_arguments: true, output: "ok\n", output_line_count: 1, truncated: false });
    renderDialog({ name: TOOL_NAME.FILE_EDIT });
    await waitFor(() => expect(screen.getByText(ARGS_NOT_KEPT_NOTE)).toBeTruthy());
    expect(document.querySelectorAll("[data-diff]")).toHaveLength(0);
    expect(screen.queryByText("Details")).toBeNull();
  });

  it("test_output_dialog_caps_its_rows", async () => {
    const output = Array.from({ length: MAX_SHOWN_ROWS + 5 }, (_, index) => `l${index}`).join("\n");
    stubRead(200, { arguments: {}, output });
    renderDialog();
    await waitFor(() => expect(numbers()).toHaveLength(MAX_SHOWN_ROWS));
    expect(screen.getByText("5 more lines; copy the output to read them all.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Copy output" })).toBeTruthy();
  });

  it("should count one hidden line as one", async () => {
    stubRead(200, { arguments: {}, output: Array.from({ length: MAX_SHOWN_ROWS + 1 }, (_, index) => `l${index}`).join("\n") });
    renderDialog();
    await waitFor(() => expect(screen.getByText("1 more line; copy the output to read it.")).toBeTruthy());
  });

  it("should draw the full output as literal text, never markup", async () => {
    const hostile = "**bold** <img src=x onerror=alert(1)>";
    stubRead(200, { arguments: {}, output: hostile });
    renderDialog();
    await waitFor(() => expect(screen.getByText(hostile)).toBeTruthy());
    expect(screen.getByRole("dialog").querySelector("img, strong")).toBeNull();
  });

  it("should give a row the thread's scope, and none outside a thread", () => {
    function Probe() {
      const scope = useFleetScope();
      return <span>{scope === null ? "none" : `${scope.workspaceId}/${scope.fleetId}`}</span>;
    }
    render(<FleetScopeProvider {...SCOPE}><Probe /></FleetScopeProvider>);
    expect(screen.getByText("ws_1/flt_1")).toBeTruthy();
    cleanup();
    render(<Probe />);
    expect(screen.getByText("none")).toBeTruthy();
  });
});

describe("keptRows", () => {
  const rows = (outcome: ToolResult | undefined) => keptRows(outcome).map(({ number, text }) => `${number ?? "⋯"}:${text}`);

  it("should number the head from 1 and the tail from where it falls", () => {
    expect(rows(SAVED)).toEqual(["1:line 1", "2:line 2", "⋯:", "223:line 223", "224:line 224"]);
  });

  it("should show a line both ends hold once, and mark no gap when there is none", () => {
    // Four lines, head holds 1-3, tail 3-4: line 3 once, no gap.
    expect(rows({ outputHead: "a\nb\nc", outputTail: "c\nd", outputLineCount: 4 })).toEqual(["1:a", "2:b", "3:c", "4:d"]);
    // Head and tail meet exactly.
    expect(rows({ outputHead: "a", outputTail: "b", outputLineCount: 2 })).toEqual(["1:a", "2:b"]);
  });

  it("should keep the head alone when the tail cannot be placed", () => {
    expect(rows({ outputHead: "a\nb", outputTail: "z" })).toEqual(["1:a", "2:b"]);
    expect(rows({ outputHead: "a\nb", outputTail: "b", outputLineCount: 2 })).toEqual(["1:a", "2:b"]);
    expect(rows({ outputHead: "a", outputLineCount: 9 })).toEqual(["1:a"]);
    expect(rows(undefined)).toEqual([]);
  });

  it("should draw the not-kept note and no rows for an outcome with nothing saved", async () => {
    stubRead(HTTP_NOT_FOUND);
    renderDialog({ outcome: {} });
    await waitFor(() => expect(screen.getByText(NOT_KEPT_NOTE)).toBeTruthy());
    expect(numbers()).toEqual([]);
  });
});
