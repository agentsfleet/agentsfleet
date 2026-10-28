import type { ReactNode } from "react";
import { Button, cn } from "@agentsfleet/design-system";

import type { ToolResult } from "./fleetReplyMessage";
import { moreLinesLabel, outputPreview } from "./tool-call-copy";
import {
  PLAN_STATUS,
  TOOL_BODY,
  type PlanStatus,
  type PlanStep,
  type ToolCopy,
} from "./tool-call-shape";
import { DIFF_ROW, type DiffRow, type LineDiff } from "./tool-call-diff";

// Recorded input previews and returned output stay in separate disclosures.
// Output is rendered as text nodes, never markup.

export const SHOW_ALL_LABEL = "show all";
export const CLIPPED_EDIT_NOTE = "Too long to diff here";
/** Under output the runner kept only the edges of: a long line, or a few. */
export const OUTPUT_CUT_LABEL = "… output continues";
export const OUTPUT_RAIL = "└";
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
// The sign and tint are for the eye; assistive tech hears the side in words.
const SPOKEN_SIDE: Record<DiffRow["kind"], string> = {
  [DIFF_ROW.ADDED]: "added: ",
  [DIFF_ROW.REMOVED]: "removed: ",
  [DIFF_ROW.CONTEXT]: "",
};
const SIGN_TONE: Record<DiffRow["kind"], string> = {
  [DIFF_ROW.ADDED]: "text-success",
  [DIFF_ROW.REMOVED]: "text-destructive",
  [DIFF_ROW.CONTEXT]: "",
};

// A plan step's state in shape and, for assistive tech, in words.
const PLAN_GLYPH: Record<PlanStatus, string> = {
  [PLAN_STATUS.PENDING]: "□",
  [PLAN_STATUS.IN_PROGRESS]: "◐",
  [PLAN_STATUS.COMPLETED]: "✔",
};
const PLAN_SPOKEN: Record<PlanStatus, string> = {
  [PLAN_STATUS.PENDING]: "to do",
  [PLAN_STATUS.IN_PROGRESS]: "in progress",
  [PLAN_STATUS.COMPLETED]: "done",
};
const PLAN_TONE: Record<PlanStatus, string> = {
  [PLAN_STATUS.PENDING]: "",
  [PLAN_STATUS.IN_PROGRESS]: "text-foreground",
  [PLAN_STATUS.COMPLETED]: "line-through",
};

/** Opens the full call; absent when there is nothing to read it by. */
type OnShowAll = (() => void) | undefined;

/** Input previews come from recorded arguments, not the tool's returned output. */
export function ToolCallInputPreview({copy, onShowAll}: {copy: ToolCopy; onShowAll: OnShowAll}) {
  const {body} = copy;
  switch (body.kind) {
    case TOOL_BODY.DIFF: return <DiffRows diff={body.diff} />;
    case TOOL_BODY.PLAN: return <PlanSteps explanation={body.explanation} steps={body.steps} />;
    case TOOL_BODY.CLIPPED_EDIT: return <RailRow glyph={OUTPUT_RAIL}>{CLIPPED_EDIT_NOTE}<ShowAll onShowAll={onShowAll} /></RailRow>;
    default: return null;
  }
}

/** Only the recorded result belongs under Output or Error. */
export function ToolCallBody({outcome, onShowAll}: {outcome: ToolResult | undefined; onShowAll: OnShowAll}) {
  return <div className="flex min-w-0 flex-col pl-md"><OutputPreview outcome={outcome} onShowAll={onShowAll} /></div>;
}

/** An edit's lines, indented under its cell, tinted by side. Exported for the
 * full-call dialog, which draws the same rows from the untrimmed edit. */
export function DiffRows({ diff }: { diff: LineDiff }) {
  return (
    <div className="flex min-w-0 flex-col pl-xl">
      {diff.rows.map((row, index) => (
        <span key={index} data-diff={row.kind} className={cn(RAIL_ROW, "px-xs", DIFF_TONE[row.kind])}>
          <span aria-hidden="true" className={cn("shrink-0", SIGN_TONE[row.kind])}>{DIFF_SIGN[row.kind]}</span>
          <span className={cn(RAIL_TEXT, "text-foreground")}>
            {SPOKEN_SIDE[row.kind] === "" ? null : <span className="sr-only">{SPOKEN_SIDE[row.kind]}</span>}
            {row.text}
          </span>
        </span>
      ))}
    </div>
  );
}

/** Codex's plan cell: the explanation, then each step with its state. */
function PlanSteps({ explanation, steps }: { explanation: string | null; steps: readonly PlanStep[] }) {
  return (
    <>
      {explanation === null ? null : <RailRow glyph={OUTPUT_RAIL}>{explanation}</RailRow>}
      {steps.map((step, index) => (
        <RailRow key={index} glyph={index === 0 && explanation === null ? OUTPUT_RAIL : null}>
          <span aria-hidden="true" data-plan-step={step.status}>{PLAN_GLYPH[step.status]}</span>{" "}
          <span className={PLAN_TONE[step.status]}>{step.step}</span>
          <span className="sr-only"> ({PLAN_SPOKEN[step.status]})</span>
        </RailRow>
      ))}
    </>
  );
}

function OutputPreview({ outcome, onShowAll }: { outcome: ToolResult | undefined; onShowAll: OnShowAll }) {
  const preview = outputPreview(outcome?.outputHead, outcome?.outputLineCount, outcome?.status, outcome?.outputTail);
  return (
    <>
      {preview.rows.map((row, index) => <RailRow key={index} glyph={index === 0 ? OUTPUT_RAIL : null}>{row}</RailRow>)}
      {preview.note === null ? null : <RailRow glyph={OUTPUT_RAIL}>{preview.note}</RailRow>}
      {preview.hiddenLines > 0 ? (
        <RailRow glyph={null}>{moreLinesLabel(preview.hiddenLines)}<ShowAll onShowAll={onShowAll} /></RailRow>
      ) : null}
      {preview.cut ? <RailRow glyph={null}>{OUTPUT_CUT_LABEL}<ShowAll onShowAll={onShowAll} /></RailRow> : null}
    </>
  );
}

/** One line under the header; a row with no glyph keeps the rail's width so
 * its text lines up with the row above. */
export function RailRow({ glyph, children }: { glyph: string | null; children: ReactNode }) {
  return (
    <span className={RAIL_ROW}>
      <span aria-hidden="true" className={cn("shrink-0 text-text-dim", glyph === null && "invisible")}>{glyph ?? OUTPUT_RAIL}</span>
      <span className={RAIL_TEXT}>{children}</span>
    </span>
  );
}

function ShowAll({ onShowAll }: { onShowAll: OnShowAll }) {
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
