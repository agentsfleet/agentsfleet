import type { FleetToolCall } from "@/lib/streaming/fleet-stream-row";
import type { PatchChange } from "./recorded-patch-changes";
import { TOOL_CALL_STATUS } from "@/lib/streaming/fleet-stream-tool-trace";
import { stringArg } from "@/components/domain/activity/tools/presentation/tool-call-text";
import { TOOL_NAME } from "@/components/domain/activity/tools/presentation/tool-call-shape";
import { recordedPatchChanges, recordedWorkspacePath } from "./recorded-patch-changes";

type ToolWrite = {path: string; running: boolean; lines: PatchChange["lines"]};
const WRITES: ReadonlySet<string> = new Set([TOOL_NAME.FILE_WRITE, TOOL_NAME.FILE_EDIT]);

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
  // The runner's file tools record each file they changed and its line counts.
  if (tool.changes?.length) return tool.changes.map(change => ({
    path: recordedWorkspacePath(change.path), running, lines: {added: change.added, removed: change.removed},
  }));
  if (tool.name === TOOL_NAME.APPLY_PATCH) return recordedPatchChanges(tool, running).map(change => ({
    ...change, running,
  }));
  if (!WRITES.has(tool.name)) return [];
  const path = recordedWorkspacePath(stringArg(tool.args, "path") ?? "");
  if (!path.trim()) return [];
  // A replacement can start or end within a line. Only a patch or repository receipt establishes file-line totals.
  return [{path, running, lines: undefined}];
}
