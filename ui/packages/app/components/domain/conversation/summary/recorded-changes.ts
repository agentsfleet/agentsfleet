import type { FleetTurn } from "@/lib/streaming/fleet-stream-row";
import { TOOL_CALL_STATUS } from "@/lib/streaming/fleet-stream-tool-trace";
import { TOOL_NAME } from "../../activity/tools/presentation/tool-call-shape";

const WRITES: ReadonlySet<string> = new Set([TOOL_NAME.FILE_WRITE, TOOL_NAME.FILE_EDIT, TOOL_NAME.FILE_EDIT_HASHED, TOOL_NAME.FILE_APPEND, TOOL_NAME.FILE_DELETE]);

export function recordedChanges(turns: readonly FleetTurn[], {includeRunning = true}: {includeRunning?: boolean} = {}): {path: string; running: boolean}[] {
  const paths = new Map<string, {path: string; running: boolean}>();
  for (const turn of turns) for (const tool of turn.tools ?? []) {
    const path = tool.args?.path;
    if (!WRITES.has(tool.name) || typeof path !== "string" || !path.trim()) continue;
    const running = !tool.done && !tool.status;
    if (running && !includeRunning) continue;
    if (!running && tool.status !== TOOL_CALL_STATUS.SUCCEEDED) continue;
    paths.delete(path);
    paths.set(path, {path, running});
  }
  return [...paths.values()].reverse();
}
