import type { MessageState } from "@assistant-ui/react";
import type { FleetToolCall } from "@/lib/streaming/fleet-stream-frames";
import { formatMs } from "@/lib/utils";

// A tool that is still running vs one that returned. Same vocabulary as the
// install ladder's state glyphs (install-flow.ts) — one glyph set, one meaning.
const TOOL_RUNNING_GLYPH = "◐";
const TOOL_DONE_GLYPH = "✓";

// The tools the fleet called while working this event. The backend has always
// published these; the stream reducer used to drop them, so the panel's own
// promise — "Tool calls, chunks, and completions appear here as the fleet runs" —
// was never kept. Narrowed rather than cast: the custom bag is `unknown`, and a
// malformed entry must not take the thread down.
export function readTools(message: MessageState): FleetToolCall[] {
  const raw = message.metadata.custom["tools"];
  if (!Array.isArray(raw)) return [];
  return raw.filter(
    (t): t is FleetToolCall =>
      typeof t === "object" &&
      t !== null &&
      typeof (t as FleetToolCall).name === "string" &&
      typeof (t as FleetToolCall).done === "boolean" &&
      ((t as FleetToolCall).ms === null || typeof (t as FleetToolCall).ms === "number"),
  );
}

// What the fleet actually DID, above what it said about it. A running tool shows
// its elapsed time so a long call reads as work rather than as a hang.
export function ToolCalls({ tools }: { tools: FleetToolCall[] }) {
  if (tools.length === 0) return null;
  return (
    <ul className="mb-xs flex flex-col gap-3xs" aria-label="Tool calls">
      {tools.map((tool, index) => (
        <li
          // Index-composed key: the reducer deliberately appends a SECOND entry
          // when the same tool is called again after the first completed, and the
          // list is append-only per event, so the index is stable.
          key={`${index}:${tool.name}`}
          data-tool={tool.name}
          data-done={tool.done || undefined}
          className="flex items-center gap-xs font-mono text-label text-muted-foreground"
        >
          <span aria-hidden="true" className={tool.done ? "text-success" : "text-pulse"}>
            {tool.done ? TOOL_DONE_GLYPH : TOOL_RUNNING_GLYPH}
          </span>
          <span>{tool.name}</span>
          {tool.ms !== null ? <span className="tabular-nums">{formatMs(tool.ms)}</span> : null}
        </li>
      ))}
    </ul>
  );
}

