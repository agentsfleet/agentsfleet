"use client";

import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import {
  CopyButton,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  TerminalPanel,
} from "@agentsfleet/design-system";

import { TOOL_CALL_READ, readToolCall, type ToolCallFull, type ToolCallRead } from "@/lib/streaming/fleet-tool-call-reader";
import { FleetPayloadDisclosure } from "./FleetPayloadDisclosure";
import { DiffRows } from "./FleetToolCallBody";
import type { ToolResult } from "./fleetReplyMessage";
import { editSides, linesOf } from "./tool-call-copy";
import { lineDiff } from "./tool-call-diff";

// "Show all": one call read in full, its output with line numbers, its edit as
// a whole diff, and its arguments. When the runner kept no full output, the
// head and tail the trace saved stand in, and the dialog says so.

export const LOADING_LABEL = "Reading the full output…";
export const NOT_KEPT_NOTE = "Full output wasn't kept for this call.";
export const READ_FAILED_NOTE = "Couldn't read the full output; this is what the thread kept.";
export const OUTPUT_CUT_NOTE = "The output was cut short.";
const OUTPUT_TITLE = "Output";
const COPY_OUTPUT_LABEL = "Copy output";
const SKIPPED_LINES = "⋯";
const NOTE_CLASS = "text-body-sm text-text-subtle";

/** The workspace and fleet a thread reads, for the reads a row makes on its own. */
export type FleetScope = { workspaceId: string; fleetId: string };

const FleetScopeContext = createContext<FleetScope | null>(null);

export function FleetScopeProvider({ workspaceId, fleetId, children }: FleetScope & { children: ReactNode }) {
  const scope = useMemo(() => ({ workspaceId, fleetId }), [workspaceId, fleetId]);
  return <FleetScopeContext value={scope}>{children}</FleetScopeContext>;
}

/** Null outside a thread: nothing there can be read in full. */
export function useFleetScope(): FleetScope | null {
  return useContext(FleetScopeContext);
}

type FleetToolOutputDialogProps = {
  scope: FleetScope;
  eventId: string;
  callId: string;
  name: string;
  /** The cell's header, verb and target, as its title. */
  title: string;
  outcome: ToolResult | undefined;
  onClose: () => void;
};

/** Mounted when "show all" is pressed, so the read starts with it and is
 * dropped with it. */
export function FleetToolOutputDialog({ scope, eventId, callId, name, title, outcome, onClose }: FleetToolOutputDialogProps) {
  const read = useToolCallRead(scope, eventId, callId);
  return (
    // Controlled open, so the only change the dialog asks for is to close.
    <Dialog open onOpenChange={onClose}>
      <DialogContent className="max-h-svh max-w-3xl overflow-y-auto">
        <DialogHeader className="border-b border-border pb-lg pr-4xl">
          <DialogTitle><span className="break-words font-mono">{title}</span></DialogTitle>
          <DialogDescription className="sr-only">{name}</DialogDescription>
        </DialogHeader>
        <div className="flex flex-col gap-lg pt-lg">
          <ReadBody read={read} name={name} outcome={outcome} />
        </div>
      </DialogContent>
    </Dialog>
  );
}

function useToolCallRead({ workspaceId, fleetId }: FleetScope, eventId: string, callId: string): ToolCallRead | null {
  const [read, setRead] = useState<ToolCallRead | null>(null);
  useEffect(() => {
    const inflight = new AbortController();
    void readToolCall({ workspaceId, fleetId, eventId, callId }, inflight.signal).then((result) => {
      if (!inflight.signal.aborted) setRead(result);
    });
    return () => inflight.abort();
  }, [workspaceId, fleetId, eventId, callId]);
  return read;
}

function ReadBody({ read, name, outcome }: { read: ToolCallRead | null; name: string; outcome: ToolResult | undefined }) {
  if (read === null) return <output className={NOTE_CLASS}>{LOADING_LABEL}</output>;
  if (read.kind === TOOL_CALL_READ.FULL) return <FullCall call={read.call} name={name} />;
  return (
    <>
      <p className={NOTE_CLASS}>{read.kind === TOOL_CALL_READ.NOT_KEPT ? NOT_KEPT_NOTE : READ_FAILED_NOTE}</p>
      <NumberedOutput rows={keptRows(outcome)} copyValue={null} />
    </>
  );
}

function FullCall({ call, name }: { call: ToolCallFull; name: string }) {
  const diff = useMemo(() => {
    const sides = editSides(name, call.args);
    return sides === null ? null : lineDiff(sides.before, sides.after);
  }, [name, call.args]);
  const rows = useMemo(() => linesOf(call.output).map((text, index) => ({ number: index + 1, text })), [call.output]);
  return (
    <>
      {diff === null ? null : <div className="font-mono text-label leading-mono"><DiffRows diff={diff} /></div>}
      <NumberedOutput rows={rows} copyValue={call.output} />
      {call.outputTruncated ? <p className={NOTE_CLASS}>{OUTPUT_CUT_NOTE}</p> : null}
      {call.args === undefined ? null : <FleetPayloadDisclosure json={JSON.stringify(call.args)} />}
    </>
  );
}

type NumberedRow = { number: number | null; text: string };

/** Lines beside their numbers; a row numbered null marks lines left out. */
function NumberedOutput({ rows, copyValue }: { rows: readonly NumberedRow[]; copyValue: string | null }) {
  return (
    <TerminalPanel
      title={OUTPUT_TITLE}
      tag={copyValue === null ? undefined : <CopyButton value={copyValue} label={COPY_OUTPUT_LABEL} />}
      bodyClassName="bg-surface-deep"
    >
      <pre className="grid grid-cols-[auto_1fr] gap-x-md p-lg font-mono text-mono leading-mono text-foreground">
        {rows.map((row, index) => (
          <NumberedLine key={index} row={row} />
        ))}
      </pre>
    </TerminalPanel>
  );
}

function NumberedLine({ row }: { row: NumberedRow }) {
  return (
    <>
      <span data-line-number={row.number ?? undefined} aria-hidden="true" className="select-none text-right tabular-nums text-text-subtle">
        {row.number ?? SKIPPED_LINES}
      </span>
      <span className="min-w-0 whitespace-pre-wrap break-words">{row.text}</span>
    </>
  );
}

/** What the trace kept: the head from line 1, then the tail where it falls,
 * the gap marked and any line both ends hold shown once. */
export function keptRows(outcome: ToolResult | undefined): NumberedRow[] {
  const head = linesOf(outcome?.outputHead ?? "");
  const tail = linesOf(outcome?.outputTail ?? "");
  const rows: NumberedRow[] = head.map((text, index) => ({ number: index + 1, text }));
  const count = outcome?.outputLineCount;
  if (count === undefined || tail.length === 0) return rows;
  const firstTail = count - tail.length + 1;
  const firstShown = Math.max(firstTail, head.length + 1);
  if (firstShown > head.length + 1) rows.push({ number: null, text: "" });
  return [...rows, ...tail.slice(firstShown - firstTail).map((text, index) => ({ number: firstShown + index, text }))];
}
