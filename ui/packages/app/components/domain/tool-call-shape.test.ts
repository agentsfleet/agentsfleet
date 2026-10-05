import { describe, expect, it } from "vitest";

import { TOOL_CALL_STATUS } from "@/lib/streaming/fleet-stream-tool-trace";
import { TOOL_BODY, showsOutput, type ToolBody } from "./tool-call-shape";

// Which settled cells show what came back, kind by kind. A call that did not
// say it succeeded always shows it; after a success, only a kind that is its
// output does.

const NOT_A_SUCCESS = [TOOL_CALL_STATUS.FAILED, TOOL_CALL_STATUS.INTERRUPTED, undefined] as const;

describe("showsOutput", () => {
  it.each<{ body: ToolBody; afterSuccess: boolean }>([
    { body: { kind: TOOL_BODY.OUTPUT }, afterSuccess: true },
    { body: { kind: TOOL_BODY.COMMAND, rail: [], hiddenLines: 0 }, afterSuccess: true },
    { body: { kind: TOOL_BODY.DIFF, diff: { rows: [], added: 0, removed: 0 } }, afterSuccess: false },
    { body: { kind: TOOL_BODY.CLIPPED_EDIT }, afterSuccess: false },
    { body: { kind: TOOL_BODY.PLAN, explanation: null, steps: [] }, afterSuccess: false },
  ])("$body.kind shows output after a success: $afterSuccess", ({ body, afterSuccess }) => {
    expect(showsOutput(body, TOOL_CALL_STATUS.SUCCEEDED)).toBe(afterSuccess);
    for (const status of NOT_A_SUCCESS) expect(showsOutput(body, status)).toBe(true);
  });
});
