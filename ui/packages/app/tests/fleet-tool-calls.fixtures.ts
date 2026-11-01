import { TOOL_NAME } from "@/components/domain/tool-call-shape";
import type { FleetToolCall } from "@/lib/streaming/fleet-stream-row";

// Pin the public action wording independently of the presenter.
export const HEADERS: ReadonlyArray<readonly [FleetToolCall["name"], FleetToolCall["args"], string, string]> = [
  [TOOL_NAME.FILE_WRITE, {path: "/workspace/notes.md", content: "a\nb\n"}, "Writing notes.md", "Wrote notes.md"],
  [TOOL_NAME.FILE_APPEND, {path: "log.txt", content: "x"}, "Appending log.txt", "Appended log.txt"],
  [TOOL_NAME.FILE_DELETE, {path: "/workspace/old.md"}, "Deleting old.md", "Deleted old.md"],
  [TOOL_NAME.HTTP_REQUEST, {method: "POST", url: "https://api.example/run"}, "Making request POST https://api.example/run", "Made request POST https://api.example/run"],
  [TOOL_NAME.HTTP_REQUEST, {url: "https://api.example/run"}, "Making request GET https://api.example/run", "Made request GET https://api.example/run"],
  [TOOL_NAME.MEMORY_STORE, {key: "deploy"}, "Saving memory deploy", "Saved memory deploy"],
  [TOOL_NAME.MEMORY_FORGET, {key: "deploy"}, "Forgetting memory deploy", "Forgot memory deploy"],
];
