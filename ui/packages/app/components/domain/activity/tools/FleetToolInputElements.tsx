import type { PlanStatus, PlanStep, ToolCopy } from "./presentation/tool-call-shape";
import { PLAN_STATUS, TOOL_BODY } from "./presentation/tool-call-shape";
import { RailRow, ShowAll, CLIPPED_EDIT_NOTE, OUTPUT_RAIL } from "./FleetToolCallBody";

const PLAN_GLYPH: Record<PlanStatus, string> = {
  [PLAN_STATUS.PENDING]: "□", [PLAN_STATUS.IN_PROGRESS]: "◐", [PLAN_STATUS.COMPLETED]: "✔",
};
const PLAN_SPOKEN: Record<PlanStatus, string> = {
  [PLAN_STATUS.PENDING]: "to do", [PLAN_STATUS.IN_PROGRESS]: "in progress", [PLAN_STATUS.COMPLETED]: "done",
};
const PLAN_TONE: Record<PlanStatus, string> = {
  [PLAN_STATUS.PENDING]: "", [PLAN_STATUS.IN_PROGRESS]: "text-foreground", [PLAN_STATUS.COMPLETED]: "line-through",
};
/** Supplement the recorded JSON where a clipped edit or a plan has its own presentation. */
export function ToolCallInputPreview({copy, onShowAll}: {copy: ToolCopy; onShowAll: (() => void) | undefined}) {
  const body = copy.body;
  if (body.kind === TOOL_BODY.CLIPPED_EDIT) return <RailRow glyph={OUTPUT_RAIL}>{CLIPPED_EDIT_NOTE}<ShowAll onShowAll={onShowAll} /></RailRow>;
  if (body.kind === TOOL_BODY.PLAN) return <PlanSteps explanation={body.explanation} steps={body.steps} />;
  return null;
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
