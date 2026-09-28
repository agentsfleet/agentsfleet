"use client";

import type { ReactNode } from "react";
import { useToolCallElapsed } from "@assistant-ui/react";
import { List, ListItem } from "@agentsfleet/design-system";
import { formatSeconds } from "@/lib/utils";

// A tool that is still running vs one that returned. Same vocabulary as the
// install ladder's state glyphs (install-flow.ts) — one glyph set, one meaning.
const TOOL_RUNNING_GLYPH = "◐";
const TOOL_DONE_GLYPH = "✓";
export const TOOL_CALLS_LABEL = "Tool calls";

/** The reply's adjacent tool calls, as one list. */
export function ToolCallList({ children }: { children: ReactNode }) {
  return (
    <List variant="plain" aria-label={TOOL_CALLS_LABEL} className="mb-xs flex flex-col gap-3xs space-y-0">
      {children}
    </List>
  );
}

/**
 * One `tool-call` part: what the fleet did, above what it said about it. The
 * clock is the library's — `useToolCallElapsed` reads the part's timing and
 * ticks only while the part runs, so a long call reads as work, not a hang,
 * and a call left unfinished on a settled reply shows no clock at all. A
 * running clock is hidden from assistive tech: inside the transcript's live
 * log, a ticking figure would be read out ten times a second.
 */
export function ToolCallRow({ name, done }: { name: string; done: boolean }) {
  const elapsedMs = useToolCallElapsed();
  return (
    <ListItem
      data-tool={name}
      data-done={done || undefined}
      className="flex items-center gap-xs font-mono text-label text-muted-foreground"
    >
      <span aria-hidden="true" className={done ? "text-success" : "text-pulse"}>
        {done ? TOOL_DONE_GLYPH : TOOL_RUNNING_GLYPH}
      </span>
      <span>{name}</span>
      {elapsedMs !== undefined ? (
        <span aria-hidden={done ? undefined : true} className="tabular-nums">{formatSeconds(elapsedMs)}</span>
      ) : null}
    </ListItem>
  );
}
