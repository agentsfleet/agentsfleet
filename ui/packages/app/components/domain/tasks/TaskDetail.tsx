"use client";

import type { DurableTaskSummary, DurableTaskDetail as NativeDurableTaskDetail } from "@/lib/api/tasks";
import type { DurableTaskResource } from "./use-task-read";
import { Button } from "@agentsfleet/design-system";
import { DurableTaskNextStep } from "./TaskNextStep";
import { useFleetDurableTaskView } from "./FleetTaskProvider";
import { DurableTaskGoalAdoption } from "./TaskGoalAdoption";
import { DurableTaskControls } from "./TaskControls";
import { DurableTaskRepositoryProgress } from "./TaskRepositoryProgress";
import { QuestionDetails } from "../setup/summary/SetupQuestions";
import { confirmed } from "./use-task-read";

type Props = {workspaceId: string; fleetId: string; task: DurableTaskSummary; onChanged: () => void};

export function DurableTaskDetailView({workspaceId, fleetId, task, onChanged, resource, retry}: Props & {resource: DurableTaskResource<NativeDurableTaskDetail>; retry: () => void}) {
  const detail = confirmed(resource);
  const readOnly = useFleetDurableTaskView()?.readOnly;
  return <div className="space-y-md pb-md text-body-sm" aria-label={`${task.title} details`}>
    {resource.kind === "loading" ? <output data-chat-loading="true" className="inline-flex items-center gap-xs text-muted-foreground">Reading saved progress…</output> : null}
    {resource.kind === "failed" ? <div className="flex items-center justify-between gap-sm text-muted-foreground">
      <p>{detail ? "Progress couldn’t be refreshed. Last confirmed details are shown." : "Saved progress is unavailable."}</p>
      <Button variant="ghost" size="sm" onClick={retry}>Retry details</Button>
    </div> : null}
    {detail ? <>
      <DurableTaskNextStep detail={detail} />
      {!readOnly ? <DurableTaskControls workspaceId={workspaceId} fleetId={fleetId} detail={detail} enabled={resource.kind === "ready"} onChanged={onChanged} /> : null}
      <DurableTaskRepositoryProgress detail={detail} />
      <div className="space-y-sm">
        <h3 className="font-medium">What counts as done?</h3>
        <p className="text-eyebrow text-muted-foreground">Uses the success criteria saved in setup v{detail.setup_revision}.</p>
        {!readOnly ? <DurableTaskGoalAdoption workspaceId={workspaceId} fleetId={fleetId} detail={detail} enabled={resource.kind === "ready"} onChanged={onChanged} /> : null}
        {detail.context.questions.map(({name, question}, index) => <QuestionDetails key={name} number={index + 1} question={{question: question.instructions, criteria: question.criteria}} />)}
      </div>
      <p className="text-eyebrow text-muted-foreground">{detail.context.usage.runs} of {detail.context.limits.runs} attempts used. {detail.context.checkpoint_id ? "Saved recovery point available." : "No recovery point has been saved."}</p>
    </> : null}
  </div>;
}
