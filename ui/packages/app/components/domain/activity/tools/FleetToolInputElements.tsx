import type { PlanStatus, PlanStep, ToolCopy } from "./presentation/tool-call-shape";
import { PLAN_STATUS, TOOL_BODY } from "./presentation/tool-call-shape";
import { ChangedFiles, RailRow, OUTPUT_RAIL } from "./FleetToolCallBody";

const PLAN_GLYPH: Record<PlanStatus, string> = {
  [PLAN_STATUS.PENDING]: "□", [PLAN_STATUS.IN_PROGRESS]: "◐", [PLAN_STATUS.COMPLETED]: "✔",
};
const PLAN_SPOKEN: Record<PlanStatus, string> = {
  [PLAN_STATUS.PENDING]: "to do", [PLAN_STATUS.IN_PROGRESS]: "in progress", [PLAN_STATUS.COMPLETED]: "done",
};
const PLAN_TONE: Record<PlanStatus, string> = {
  [PLAN_STATUS.PENDING]: "", [PLAN_STATUS.IN_PROGRESS]: "text-foreground", [PLAN_STATUS.COMPLETED]: "line-through",
};
/** Supplement the recorded JSON where an edit's recorded changes or a plan has its own presentation.
 * Input describes the requested action; its renderer never establishes success. */
export function ToolCallInputPreview({copy}: {copy: ToolCopy}) {
  const body = copy.body;
  if (body.kind === TOOL_BODY.CHANGES) return <ChangedFiles changes={body.changes} />;
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
