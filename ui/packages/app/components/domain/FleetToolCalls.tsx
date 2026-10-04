"use client";

import { memo, useMemo, useState, type ReactNode } from "react";
import { useToolCallElapsed } from "@assistant-ui/react";
import { List, ListItem, cn } from "@agentsfleet/design-system";

import type { ToolArgs } from "@/lib/streaming/fleet-stream-tool-trace";
import { formatSeconds } from "@/lib/utils";
import { FleetPayloadDisclosure } from "./FleetPayloadDisclosure";
import { ToolCallBody } from "./FleetToolCallBody";
import { FleetToolOutputDialog, useFleetScope } from "./FleetToolOutputDialog";
import { readToolResult } from "./fleetReplyMessage";
import { CELL_STATE, FAILED_MARK, TOOL_BODY, cellState, toolCopy, verbFor, type CellState } from "./tool-call-copy";
import { lineDiff } from "./tool-call-diff";

// A tool call as Codex's transcript draws one: a status bullet, a bold verb
// and its target, dim figures, and what came back under a rail. The words come
// from the pure copy map; this file only lays them out.

export const TOOL_CALLS_LABEL = "Tool calls";
export const TOOL_BULLET = "•";
const FIGURE_SEPARATOR = " · ";
const MINUS = "−";

const BULLET_TONE: Record<CellState, string> = {
  [CELL_STATE.RUNNING]: "text-text-dim",
  [CELL_STATE.SUCCEEDED]: "text-success",
  [CELL_STATE.FAILED]: "text-destructive",
  [CELL_STATE.INTERRUPTED]: "text-destructive",
  [CELL_STATE.SETTLED]: "text-text-dim",
};

/** The reply's adjacent tool calls, as one list. */
export function ToolCallList({ children }: { children: ReactNode }) {
  return (
    <List variant="plain" aria-label={TOOL_CALLS_LABEL} className="mb-xs flex flex-col gap-xs space-y-0">
      {children}
    </List>
  );
}

export type ToolCallRowProps = {
  name: string;
  args: ToolArgs;
  argsText: string;
  result: unknown;
  /** The part is still running: its turn runs and it has no result yet. */
  running: boolean;
  eventId: string;
};

/**
 * One `tool-call` part. The figures and the body memo on the part's own
 * fields, so a sibling's clock tick or a streamed word re-renders neither; the
 * clock is a leaf of its own for the same reason.
 */
export const ToolCallRow = memo(function ToolCallRow({ name, args, argsText, result, running, eventId }: ToolCallRowProps) {
  const named = Object.keys(args).length > 0 ? args : undefined;
  const copy = useMemo(() => toolCopy(name, named), [name, named]);
  const outcome = readToolResult(result);
  const state = cellState(outcome !== undefined, outcome?.status, running);
  const diff = useMemo(
    () => (copy.body.kind === TOOL_BODY.EDIT ? lineDiff(copy.body.before, copy.body.after) : null),
    [copy],
  );
  const [shown, setShown] = useState(false);
  const scope = useFleetScope();
  const callId = outcome?.callId;
  const verb = verbFor(copy.verbs, state);
  // Only a call the runner named can be read in full, and only inside a thread.
  const canShowAll = scope !== null && callId !== undefined;
  return (
    <ListItem
      data-tool={name}
      data-done={outcome !== undefined || undefined}
      data-state={state}
      className="flex min-w-0 flex-col gap-3xs font-mono text-label leading-mono text-text-dim"
    >
      <span className="flex min-w-0 items-baseline gap-xs">
        <span aria-hidden="true" data-tool-shimmer={state === CELL_STATE.RUNNING} className={cn("transition-colors duration-snap ease-snap", BULLET_TONE[state])}>
          {TOOL_BULLET}
        </span>
        <span className="min-w-0 break-words">
          <span className="font-semibold text-foreground">{verb}</span>{" "}
          <span className="text-foreground">{copy.target}</span>
          {copy.addedLines !== undefined ? <> <span className="text-success">(+{copy.addedLines})</span></> : null}
          {diff !== null ? <> (<span className="text-success">+{diff.added}</span> <span className="text-destructive">{MINUS}{diff.removed}</span>)</> : null}
          <HeaderMark exitCode={outcome?.exitCode} failed={state === CELL_STATE.FAILED} />
          <CallClock running={state === CELL_STATE.RUNNING} />
        </span>
      </span>
      {state === CELL_STATE.RUNNING ? null : (
        <ToolCallBody copy={copy} diff={diff} outcome={outcome} onShowAll={canShowAll ? () => setShown(true) : undefined} />
      )}
      {named === undefined ? null : <FleetPayloadDisclosure json={argsText} />}
      {shown && canShowAll ? (
        <FleetToolOutputDialog
          scope={scope}
          eventId={eventId}
          callId={callId}
          name={name}
          title={`${verb} ${copy.target}`.trim()}
          outcome={outcome}
          onClose={() => setShown(false)}
        />
      ) : null}
    </ListItem>
  );
});

/** A non-zero exit says how a command failed; any other failure says so in
 * words, so the outcome never rests on the bullet's colour alone. */
function HeaderMark({ exitCode, failed }: { exitCode: number | undefined; failed: boolean }) {
  if (exitCode !== undefined && exitCode !== 0) return <span className="text-destructive"> (exit {exitCode})</span>;
  return failed ? <span className="text-destructive"> {FAILED_MARK}</span> : null;
}

/**
 * The library's clock: it reads the part's timing and ticks only while the
 * part runs, and a call its turn left unfinished shows none. A running clock
 * is hidden from assistive tech: inside the transcript's live log, a ticking
 * figure would be read out on every tick.
 */
function CallClock({ running }: { running: boolean }) {
  const elapsedMs = useToolCallElapsed();
  if (elapsedMs === undefined) return null;
  return (
    <span aria-hidden={running || undefined} className="tabular-nums">
      {FIGURE_SEPARATOR}
      {formatSeconds(elapsedMs)}
    </span>
  );
}
