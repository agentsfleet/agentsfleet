import type { ReactNode } from "react";
import { Button, cn } from "@agentsfleet/design-system";

import { TOOL_CALL_STATUS } from "@/lib/streaming/fleet-stream-tool-trace";
import type { ToolResult } from "./fleetReplyMessage";
import { TOOL_BODY, moreLinesLabel, outputPreview, type ToolCopy } from "./tool-call-copy";
import { DIFF_ROW, type DiffRow, type LineDiff } from "./tool-call-diff";

// What sits under a tool cell's header, as Codex lays it out: a command's
// further lines on a `│` rail, an edit's diff, and the output's first rows
// under `└` with what they left out. Output is text nodes, never markup.

export const SHOW_ALL_LABEL = "show all";
export const CLIPPED_EDIT_NOTE = "Too long to diff here";
export const OUTPUT_RAIL = "└";
const COMMAND_RAIL = "│";
const MORE_COMMAND = "…";
const RAIL_ROW = "flex min-w-0 gap-xs";
const RAIL_TEXT = "min-w-0 whitespace-pre-wrap break-words";

const DIFF_SIGN: Record<DiffRow["kind"], string> = {
  [DIFF_ROW.ADDED]: "+",
  [DIFF_ROW.REMOVED]: "-",
  [DIFF_ROW.CONTEXT]: " ",
};
const DIFF_TONE: Record<DiffRow["kind"], string> = {
  [DIFF_ROW.ADDED]: "bg-success/10",
  [DIFF_ROW.REMOVED]: "bg-destructive/10",
  [DIFF_ROW.CONTEXT]: "",
};
const SIGN_TONE: Record<DiffRow["kind"], string> = {
  [DIFF_ROW.ADDED]: "text-success",
  [DIFF_ROW.REMOVED]: "text-destructive",
  [DIFF_ROW.CONTEXT]: "",
};

type ToolCallBodyProps = {
  copy: ToolCopy;
  diff: LineDiff | null;
  outcome: ToolResult | undefined;
  /** Opens the full call; absent when there is nothing to read it by. */
  onShowAll: (() => void) | undefined;
};

/** A settled cell's body. An edit that worked shows its diff alone, as Codex's
 * does; anything that did not shows what came back. */
export function ToolCallBody({ copy, diff, outcome, onShowAll }: ToolCallBodyProps) {
  const { body } = copy;
  const showsOutput = body.kind === TOOL_BODY.OUTPUT || body.kind === TOOL_BODY.COMMAND
    || outcome?.status !== TOOL_CALL_STATUS.SUCCEEDED;
  return (
    <div className="flex min-w-0 flex-col pl-md">
      {body.kind === TOOL_BODY.COMMAND ? <CommandRail rail={body.rail} hiddenLines={body.hiddenLines} /> : null}
      {diff === null ? null : <DiffRows diff={diff} />}
      {body.kind === TOOL_BODY.CLIPPED_EDIT ? (
        <RailRow glyph={OUTPUT_RAIL}>{CLIPPED_EDIT_NOTE}<ShowAll onShowAll={onShowAll} /></RailRow>
      ) : null}
      {showsOutput ? <OutputPreview outcome={outcome} onShowAll={onShowAll} /> : null}
    </div>
  );
}

/** An edit's lines, indented under its cell, tinted by side. Exported for the
 * full-call dialog, which draws the same rows from the untrimmed edit. */
export function DiffRows({ diff }: { diff: LineDiff }) {
  return (
    <div className="flex min-w-0 flex-col pl-xl">
      {diff.rows.map((row, index) => (
        <span key={index} data-diff={row.kind} className={cn(RAIL_ROW, "px-xs", DIFF_TONE[row.kind])}>
          <span aria-hidden="true" className={cn("shrink-0", SIGN_TONE[row.kind])}>{DIFF_SIGN[row.kind]}</span>
          <span className={cn(RAIL_TEXT, "text-foreground")}>{row.text}</span>
        </span>
      ))}
    </div>
  );
}

function CommandRail({ rail, hiddenLines }: { rail: readonly string[]; hiddenLines: number }) {
  return (
    <>
      {rail.map((line, index) => <RailRow key={index} glyph={COMMAND_RAIL}>{line}</RailRow>)}
      {hiddenLines > 0 ? <RailRow glyph={COMMAND_RAIL}>{MORE_COMMAND} {moreLinesLabel(hiddenLines)}</RailRow> : null}
    </>
  );
}

function OutputPreview({ outcome, onShowAll }: { outcome: ToolResult | undefined; onShowAll: (() => void) | undefined }) {
  const preview = outputPreview(outcome?.outputHead, outcome?.outputLineCount, outcome?.status);
  return (
    <>
      {preview.rows.map((row, index) => <RailRow key={index} glyph={index === 0 ? OUTPUT_RAIL : null}>{row}</RailRow>)}
      {preview.note === null ? null : <RailRow glyph={OUTPUT_RAIL}>{preview.note}</RailRow>}
      {preview.hiddenLines > 0 ? (
        <RailRow glyph={null}>{moreLinesLabel(preview.hiddenLines)}<ShowAll onShowAll={onShowAll} /></RailRow>
      ) : null}
    </>
  );
}

/** One line under the header; a row with no glyph keeps the rail's width so
 * its text lines up with the row above. */
export function RailRow({ glyph, children }: { glyph: string | null; children: ReactNode }) {
  return (
    <span className={RAIL_ROW}>
      <span aria-hidden="true" className={cn("shrink-0", glyph === null && "invisible")}>{glyph ?? OUTPUT_RAIL}</span>
      <span className={RAIL_TEXT}>{children}</span>
    </span>
  );
}

function ShowAll({ onShowAll }: { onShowAll: (() => void) | undefined }) {
  if (onShowAll === undefined) return null;
  return (
    <>
      {" "}
      <Button type="button" variant="link" size="eyebrow" className="px-0 text-label" onClick={onShowAll}>
        {SHOW_ALL_LABEL}
      </Button>
    </>
  );
}
