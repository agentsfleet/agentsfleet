import type { ReactNode } from "react";
import type { PlanStatus, PlanStep, ToolBodyKind, ToolBodyOf, ToolCopy } from "./tool-call-shape";
import { PLAN_STATUS, TOOL_BODY } from "./tool-call-shape";
import { DiffRows, RailRow, ShowAll, CLIPPED_EDIT_NOTE, OUTPUT_RAIL } from "./FleetToolCallBody";

type InputContext = {onShowAll: (() => void) | undefined};
type InputRenderers = {[K in ToolBodyKind]: (body: ToolBodyOf<K>, context: InputContext) => ReactNode};

const PLAN_GLYPH: Record<PlanStatus, string> = {
  [PLAN_STATUS.PENDING]: "□", [PLAN_STATUS.IN_PROGRESS]: "◐", [PLAN_STATUS.COMPLETED]: "✔",
};
const PLAN_SPOKEN: Record<PlanStatus, string> = {
  [PLAN_STATUS.PENDING]: "to do", [PLAN_STATUS.IN_PROGRESS]: "in progress", [PLAN_STATUS.COMPLETED]: "done",
};
const PLAN_TONE: Record<PlanStatus, string> = {
  [PLAN_STATUS.PENDING]: "", [PLAN_STATUS.IN_PROGRESS]: "text-foreground", [PLAN_STATUS.COMPLETED]: "line-through",
};
const INPUT_RENDERERS: Readonly<InputRenderers> = Object.freeze({
  // Generic tools already show their recorded JSON input in ToolCallDetails.
  [TOOL_BODY.OUTPUT]: () => null,
  [TOOL_BODY.COMMAND]: () => null,
  [TOOL_BODY.DIFF]: body => <DiffRows diff={body.diff} />,
  [TOOL_BODY.PLAN]: body => <PlanSteps explanation={body.explanation} steps={body.steps} />,
  [TOOL_BODY.CLIPPED_EDIT]: (_body, context) => <RailRow glyph={OUTPUT_RAIL}>{CLIPPED_EDIT_NOTE}<ShowAll onShowAll={context.onShowAll} /></RailRow>,
});

/** Input describes the requested action; its renderer never establishes success. */
export function ToolCallInputPreview({copy, onShowAll}: {copy: ToolCopy; onShowAll: InputContext["onShowAll"]}) {
  return renderInput(copy.body.kind, copy.body, {onShowAll});
}

function renderInput<K extends ToolBodyKind>(kind: K, body: ToolBodyOf<K>, context: InputContext) {
  return INPUT_RENDERERS[kind](body, context);
}

function PlanSteps({explanation, steps}: {explanation: string | null; steps: readonly PlanStep[]}) {
  return <>
    {explanation === null ? null : <RailRow glyph={OUTPUT_RAIL}>{explanation}</RailRow>}
    {steps.map((step, index) => <RailRow key={index} glyph={index === 0 && explanation === null ? OUTPUT_RAIL : null}>
      <span aria-hidden="true" data-plan-step={step.status}>{PLAN_GLYPH[step.status]}</span>{" "}
      <span className={PLAN_TONE[step.status]}>{step.step}</span>
      <span className="sr-only"> ({PLAN_SPOKEN[step.status]})</span>
    </RailRow>)}
  </>;
}
