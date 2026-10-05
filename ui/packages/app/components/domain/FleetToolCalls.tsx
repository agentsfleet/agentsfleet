"use client";

import { memo, useMemo, useState, type ReactNode } from "react";
import { useToolCallElapsed } from "@assistant-ui/react";
import { List, ListItem, cn } from "@agentsfleet/design-system";

import type { ToolArgs } from "@/lib/streaming/fleet-stream-tool-trace";
import { formatSeconds } from "@/lib/utils";
import { FleetPayloadDisclosure } from "./FleetPayloadDisclosure";
import { ToolCallBody } from "./FleetToolCallBody";
import { useFleetScope } from "./FleetScope";
import { FleetToolOutputDialog } from "./FleetToolOutputDialog";
import { readToolResult } from "./fleetReplyMessage";
import { CELL_STATE, FAILED_MARK, cellState, toolCopy, verbFor, type CellState } from "./tool-call-copy";
import { TOOL_BODY, type ToolCopy } from "./tool-call-shape";
import type { ToolResult } from "./fleetReplyMessage";
import { lineDiff, type LineDiff } from "./tool-call-diff";

// A tool call as Codex's transcript draws one: a status bullet, a bold verb
// and its target, dim figures, and what came back under a rail. The words come
// from the pure copy map; this file only lays them out.

export const TOOL_CALLS_LABEL = "Tool calls";
export const TOOL_BULLET = "•";
// A call the runner confirmed went well says so in shape as well as colour.
export const TOOL_SUCCEEDED_GLYPH = "✓";
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
  /** The turn has ended, so the runner has posted every call's full record. */
  settled: boolean;
  eventId: string;
};

/**
 * One `tool-call` part. It memos on the part's own fields, which keep their
 * identity while the reply streams (`toolCallPart`), so a streamed word or a
 * sibling's clock tick re-renders neither; the clock is a leaf of its own.
 */
export const ToolCallRow = memo(function ToolCallRow({ name, args, argsText, result, running, settled, eventId }: ToolCallRowProps) {
  const named = Object.keys(args).length > 0 ? args : undefined;
  const copy = useMemo(() => toolCopy(name, named), [name, named]);
  const outcome = readToolResult(result);
  const state = cellState(outcome !== undefined, outcome?.status, running);
  const diff = useMemo(
    () => {
      if (copy.body.kind === TOOL_BODY.EDIT) return lineDiff(copy.body.before, copy.body.after);
      return copy.body.kind === TOOL_BODY.PATCH ? copy.body.diff : null;
    },
    [copy],
  );
  // A row is keyed by its call's own id (`toolCallPart`), so a saved trace that
  // puts another call in this place mounts a new row, and an open dialog goes.
  const [shown, setShown] = useState(false);
  const scope = useFleetScope();
  const callId = outcome?.callId;
  const verb = verbFor(copy.verbs, state);
  // Only a call the runner named can be read in full, only inside a thread,
  // and only once its turn has ended: the runner posts full records at settle.
  const canShowAll = scope !== null && callId !== undefined && settled;
  // A reply that runs again takes its dialog with it, and settling again does
  // not bring it back unasked.
  if (shown && !canShowAll) setShown(false);
  return (
    <ListItem
      data-tool={name}
      data-done={outcome !== undefined || undefined}
      data-state={state}
      className="flex min-w-0 flex-col gap-xs font-mono text-label leading-mono text-text-subtle"
    >
      <ToolCallHeader verb={verb} copy={copy} diff={diff} outcome={outcome} state={state} />
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
          verb={verb}
          target={copy.target}
          outcome={outcome}
          onClose={() => setShown(false)}
        />
      ) : null}
    </ListItem>
  );
});

/** The bullet, the verb and what it touched, the line counts, how it ended,
 * and how long it took. */
function ToolCallHeader({ verb, copy, diff, outcome, state }: {
  verb: string;
  copy: ToolCopy;
  diff: LineDiff | null;
  outcome: ToolResult | undefined;
  state: CellState;
}) {
  return (
    <span className="flex min-w-0 items-baseline gap-xs">
      <span aria-hidden="true" data-tool-bullet="" data-tool-shimmer={state === CELL_STATE.RUNNING} className={cn("transition-colors duration-snap ease-snap", BULLET_TONE[state])}>
        {state === CELL_STATE.SUCCEEDED ? TOOL_SUCCEEDED_GLYPH : TOOL_BULLET}
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
  );
}

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
