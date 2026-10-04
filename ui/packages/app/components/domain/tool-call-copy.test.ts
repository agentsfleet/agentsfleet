import { describe, expect, it } from "vitest";

import { TOOL_CALL_STATUS } from "@/lib/streaming/fleet-stream-tool-trace";
import {
  ARGS_LEAF_MAX_BYTES,
  CELL_STATE,
  INTERRUPTED_VERB,
  EMPTY_OUTPUT,
  OUTPUT_UNAVAILABLE,
  TOOL_BODY,
  TOOL_NAME,
  cellState,
  mayBeClipped,
  moreLinesLabel,
  outputPreview,
  editSides,
  toolCopy,
  verbFor,
  workspacePath,
} from "./tool-call-copy";

const PATH = "/workspace/deploy.yaml";
const SHOWN_PATH = "deploy.yaml";
const URL = "https://api.example.test/v1/health";
const KEY = "deploy_window";
const EIGHT_LINES = "1\n2\n3\n4\n5\n6\n7\n8\n";
const CLIPPED = "x".repeat(ARGS_LEAF_MAX_BYTES);

function headerOf(name: string, args: Record<string, string> | undefined, state: (typeof CELL_STATE)[keyof typeof CELL_STATE]) {
  const copy = toolCopy(name, args);
  return `${verbFor(copy.verbs, state)} ${copy.target}`;
}

describe("toolCopy", () => {
  it("names each hosted tool's verb and target, running and done", () => {
    const cases = [
      [TOOL_NAME.FILE_WRITE, { path: PATH, content: "a\nb\n" }, "Writing deploy.yaml", "Wrote deploy.yaml"],
      [TOOL_NAME.FILE_APPEND, { path: PATH, content: "a" }, "Appending deploy.yaml", "Appended deploy.yaml"],
      [TOOL_NAME.FILE_DELETE, { path: PATH }, "Deleting deploy.yaml", "Deleted deploy.yaml"],
      [TOOL_NAME.FILE_EDIT, { path: PATH, old_text: "a", new_text: "b" }, "Editing deploy.yaml", "Edited deploy.yaml"],
      [TOOL_NAME.HTTP_REQUEST, { url: URL, method: "POST" }, `Requesting POST ${URL}`, `Requested POST ${URL}`],
      [TOOL_NAME.MEMORY_STORE, { key: KEY }, `Remembering ${KEY}`, `Remembered ${KEY}`],
      [TOOL_NAME.MEMORY_FORGET, { key: KEY }, `Forgetting ${KEY}`, `Forgot ${KEY}`],
      [TOOL_NAME.EXEC_COMMAND, { cmd: "ls -la" }, "Running ls -la", "Ran ls -la"],
      [TOOL_NAME.SHELL, { command: "make test" }, "Running make test", "Ran make test"],
    ] as const;
    for (const [name, args, running, done] of cases) {
      expect(headerOf(name, args, CELL_STATE.RUNNING)).toBe(running);
      expect(headerOf(name, args, CELL_STATE.SUCCEEDED)).toBe(done);
    }
  });

  it("requests GET when the call names no method, and reads missing targets as empty", () => {
    expect(toolCopy(TOOL_NAME.HTTP_REQUEST, { url: URL }).target).toBe(`GET ${URL}`);
    expect(toolCopy(TOOL_NAME.MEMORY_STORE, undefined).target).toBe("");
    expect(toolCopy(TOOL_NAME.MEMORY_FORGET, {}).target).toBe("");
    expect(toolCopy(TOOL_NAME.FILE_DELETE, { path: 7 }).target).toBe("");
  });

  it("calls an unknown tool by name, its arguments compact and bounded", () => {
    expect(toolCopy("fly_status", { app: "x" }).target).toBe('fly_status({"app":"x"})');
    // An inherited property name is no tool this map knows.
    expect(toolCopy("toString", undefined).target).toBe("toString");
    expect(toolCopy("fly_status", undefined).target).toBe("fly_status");
    expect(toolCopy("fly_status", { note: "y".repeat(500) }).target.endsWith("…)")).toBe(true);
  });

  it("names an edit's two sides for a full read, and none for any other tool", () => {
    expect(editSides(TOOL_NAME.FILE_EDIT_HASHED, { old_text: CLIPPED, new_text: "b" })).toEqual({ before: CLIPPED, after: "b" });
    expect(editSides(TOOL_NAME.FILE_EDIT, undefined)).toEqual({ before: "", after: "" });
    expect(editSides(TOOL_NAME.FILE_WRITE, { old_text: "a" })).toBeNull();
  });

  it("replaces the verb with Interrupted, whatever the tool", () => {
    expect(headerOf(TOOL_NAME.FILE_WRITE, { path: PATH }, CELL_STATE.INTERRUPTED)).toBe(`${INTERRUPTED_VERB} ${SHOWN_PATH}`);
  });

  it("counts a write's lines only when its content arrived whole", () => {
    expect(toolCopy(TOOL_NAME.FILE_WRITE, { path: PATH, content: "a\nb\n" }).addedLines).toBe(2);
    expect(toolCopy(TOOL_NAME.FILE_WRITE, { path: PATH, content: CLIPPED }).addedLines).toBeUndefined();
    expect(toolCopy(TOOL_NAME.FILE_WRITE, { path: PATH }).addedLines).toBeUndefined();
  });

  it("diffs an edit whole, and refuses to diff one the runner may have cut", () => {
    expect(toolCopy(TOOL_NAME.FILE_EDIT_HASHED, { path: PATH, old_text: "a", new_text: "b" }).body)
      .toEqual({ kind: TOOL_BODY.EDIT, before: "a", after: "b" });
    expect(toolCopy(TOOL_NAME.FILE_EDIT, { path: PATH, old_text: CLIPPED, new_text: "b" }).body).toEqual({ kind: TOOL_BODY.CLIPPED_EDIT });
    expect(toolCopy(TOOL_NAME.FILE_EDIT, { path: PATH }).body).toEqual({ kind: TOOL_BODY.EDIT, before: "", after: "" });
  });

  it("puts a command's first line on the header, two more on the rail, and counts the rest", () => {
    expect(toolCopy(TOOL_NAME.EXEC_COMMAND, { cmd: "a\nb\nc\nd" })).toMatchObject({
      target: "a",
      body: { kind: TOOL_BODY.COMMAND, rail: ["b", "c"], hiddenLines: 1 },
    });
    expect(toolCopy(TOOL_NAME.SHELL, {}).body).toEqual({ kind: TOOL_BODY.COMMAND, rail: [], hiddenLines: 0 });
  });
});

describe("cellState", () => {
  it("reads running, interrupted, the reported outcome, or settled with none", () => {
    expect(cellState(false, undefined, true)).toBe(CELL_STATE.RUNNING);
    // No result on a part that stopped running: its turn ended it.
    expect(cellState(false, undefined, false)).toBe(CELL_STATE.INTERRUPTED);
    expect(cellState(true, TOOL_CALL_STATUS.FAILED, false)).toBe(CELL_STATE.FAILED);
    expect(cellState(true, undefined, false)).toBe(CELL_STATE.SETTLED);
  });
});

describe("outputPreview", () => {
  it("keeps three rows and counts the lines it left out", () => {
    expect(outputPreview(EIGHT_LINES, 8, TOOL_CALL_STATUS.SUCCEEDED)).toEqual({ rows: ["1", "2", "3"], hiddenLines: 5, note: null });
    // Without a count, the head's own lines are the whole output.
    expect(outputPreview("a\nb", undefined, TOOL_CALL_STATUS.SUCCEEDED)).toEqual({ rows: ["a", "b"], hiddenLines: 0, note: null });
  });

  it("says there was no output, or that none was kept", () => {
    expect(outputPreview(undefined, 0, TOOL_CALL_STATUS.SUCCEEDED).note).toBe(EMPTY_OUTPUT);
    expect(outputPreview(undefined, undefined, undefined).note).toBe(OUTPUT_UNAVAILABLE);
    // Cut off before it reported: nothing says it printed nothing.
    expect(outputPreview(undefined, undefined, TOOL_CALL_STATUS.INTERRUPTED).note).toBe(OUTPUT_UNAVAILABLE);
    expect(outputPreview("", 0, TOOL_CALL_STATUS.FAILED).note).toBe(EMPTY_OUTPUT);
    expect(outputPreview(undefined, 12, TOOL_CALL_STATUS.SUCCEEDED)).toEqual({ rows: [], hiddenLines: 12, note: OUTPUT_UNAVAILABLE });
  });

  it("labels one hidden line in the singular", () => {
    expect(moreLinesLabel(1)).toBe("+1 line");
    expect(moreLinesLabel(5)).toBe("+5 lines");
  });
});

describe("argument helpers", () => {
  it("treats a string at the cut length as possibly clipped, counting bytes", () => {
    expect(mayBeClipped("x".repeat(ARGS_LEAF_MAX_BYTES - 4))).toBe(false);
    expect(mayBeClipped("x".repeat(ARGS_LEAF_MAX_BYTES - 3))).toBe(true);
    // 85 three-byte characters: 255 bytes, cut at the boundary below 256.
    expect(mayBeClipped("€".repeat(85))).toBe(true);
  });

  it("shows a sandbox path relative to the workspace", () => {
    expect(workspacePath(PATH)).toBe(SHOWN_PATH);
    expect(workspacePath("/etc/hosts")).toBe("/etc/hosts");
    // HTTP output ends its lines in CRLF; a row must not carry the carriage return.
    expect(outputPreview("HTTP/1.1 200 OK\r\nok\r\n", 2, TOOL_CALL_STATUS.SUCCEEDED).rows).toEqual(["HTTP/1.1 200 OK", "ok"]);
  });
});
