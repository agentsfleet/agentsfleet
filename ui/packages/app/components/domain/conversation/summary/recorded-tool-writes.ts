import type { FleetToolCall } from "@/lib/streaming/fleet-stream-row";
import type { PatchChange } from "./recorded-patch-changes";
import { TOOL_CALL_STATUS } from "@/lib/streaming/fleet-stream-tool-trace";
import { lineDiff } from "@/components/domain/activity/tools/diff/tool-call-diff";
import { ARGS_LEAF_MAX_BYTES, mayBeClipped, stringArg } from "@/components/domain/activity/tools/presentation/tool-call-text";
import { TOOL_NAME } from "@/components/domain/activity/tools/presentation/tool-call-shape";
import { recordedPatchChanges } from "./recorded-patch-changes";

type ToolWrite = {path: string; running: boolean; totals: () => PatchChange["lines"]};
const WRITES: ReadonlySet<string> = new Set([TOOL_NAME.FILE_WRITE, TOOL_NAME.FILE_EDIT, TOOL_NAME.FILE_EDIT_HASHED, TOOL_NAME.FILE_APPEND, TOOL_NAME.FILE_DELETE]);

/** Stream reducers replace changed calls; weak keys retain neither old turns nor repository context. */
export class RecordedToolWrites {
  #recordings = new WeakMap<FleetToolCall, readonly ToolWrite[]>();

  read(tool: FleetToolCall): readonly ToolWrite[] {
    const cached = this.#recordings.get(tool);
    if (cached) return cached;
    const running = !tool.done && !tool.status;
    const writes = !running && tool.status !== TOOL_CALL_STATUS.SUCCEEDED ? [] : normalize(tool, running);
    this.#recordings.set(tool, writes);
    return writes;
  }
}

function normalize(tool: FleetToolCall, running: boolean): ToolWrite[] {
  if (tool.name === TOOL_NAME.APPLY_PATCH) return recordedPatchChanges(tool, running).map(change => ({
    path: change.path, running, totals: () => change.lines,
  }));
  if (!WRITES.has(tool.name)) return [];
  const path = stringArg(tool.args, "path");
  if (path === undefined || !path.trim()) return [];
  // An index receipt can avoid reading a diff entirely; an unknown result is cached too.
  let totals: {value: PatchChange["lines"]} | undefined;
  return [{path, running, totals: () => (totals ??= {value: editTotals(tool)}).value}];
}

function editTotals(tool: FleetToolCall): PatchChange["lines"] {
  if (tool.name !== TOOL_NAME.FILE_EDIT) return undefined;
  const before = stringArg(tool.args, "old_text"), after = stringArg(tool.args, "new_text");
  if (before === undefined || after === undefined) return undefined;
  // Reject a large value before encoding it; real recorded leaves are bounded at this cap.
  if (before.length > ARGS_LEAF_MAX_BYTES || after.length > ARGS_LEAF_MAX_BYTES || mayBeClipped(before) || mayBeClipped(after)) return undefined;
  const {added, removed} = lineDiff(before, after);
  return {added, removed};
}
