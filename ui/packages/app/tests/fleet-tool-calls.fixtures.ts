import { TOOL_NAME } from "@/components/domain/tool-call-shape";
import type { FleetToolCall } from "@/lib/streaming/fleet-stream-row";

// Pin the public action wording independently of the presenter.
export const HEADERS: ReadonlyArray<readonly [FleetToolCall["name"], FleetToolCall["args"], string, string]> = [
  [TOOL_NAME.FILE_WRITE, {path: "/workspace/notes.md", content: "a\nb\n"}, "Writing file", "Wrote file"],
  [TOOL_NAME.FILE_APPEND, {path: "log.txt", content: "x"}, "Appending file", "Appended file"],
  [TOOL_NAME.FILE_DELETE, {path: "/workspace/old.md"}, "Deleting file", "Deleted file"],
  [TOOL_NAME.HTTP_REQUEST, {method: "POST", url: "https://api.example/run"}, "Making request", "Made request"],
  [TOOL_NAME.HTTP_REQUEST, {url: "https://api.example/run"}, "Making request", "Made request"],
  [TOOL_NAME.MEMORY_STORE, {key: "deploy"}, "Saving memory", "Saved memory"],
  [TOOL_NAME.MEMORY_FORGET, {key: "deploy"}, "Forgetting memory", "Forgot memory"],
];
