"use client";

import type { TaskSummary, TaskDetail as NativeTaskDetail } from "@/lib/api/tasks";
import type { TaskResource } from "./use-task-read";
import { useCallback } from "react";
import { Button, SignalSpinner } from "@agentsfleet/design-system";
import { readTask } from "@/lib/api/tasks";
import { TaskNextStep } from "./TaskNextStep";
import { TaskGoalAdoption } from "./TaskGoalAdoption";
import { TaskControls } from "./TaskControls";
import { TaskRepositoryProgress } from "./TaskRepositoryProgress";
import { QuestionDetails } from "./SetupQuestions";
import { confirmed, useTaskRead } from "./use-task-read";

type Props = {workspaceId: string; fleetId: string; task: TaskSummary; onChanged: () => void};

export function TaskDetail({workspaceId, fleetId, task, onChanged}: Props) {
  // A refreshed summary must reread dependency observations even when its task version is unchanged.
  const load = useCallback((signal: AbortSignal) => readTask(workspaceId, fleetId, task.id, signal), [workspaceId, fleetId, task]);
  const {resource, retry} = useTaskRead(load, task.resource_version);
  return <TaskDetailView workspaceId={workspaceId} fleetId={fleetId} task={task} onChanged={() => {retry(); onChanged();}} resource={resource} retry={retry} />;
}

export function TaskDetailView({workspaceId, fleetId, task, onChanged, resource, retry}: Props & {resource: TaskResource<NativeTaskDetail>; retry: () => void}) {
  const detail = confirmed(resource);
  return <div className="space-y-md pb-md text-body-sm" aria-label={`${task.title} details`}>
    {resource.kind === "loading" ? <output className="inline-flex items-center gap-xs text-muted-foreground"><SignalSpinner aria-hidden="true" />Reading saved progress…</output> : null}
    {resource.kind === "failed" ? <div className="flex items-center justify-between gap-sm text-muted-foreground">
      <p>{detail ? "Progress couldn’t be refreshed. Last confirmed details are shown." : "Saved progress is unavailable."}</p>
      <Button variant="ghost" size="sm" onClick={retry}>Retry details</Button>
    </div> : null}
    {detail ? <>
      <TaskNextStep detail={detail} />
      <TaskControls workspaceId={workspaceId} fleetId={fleetId} detail={detail} enabled={resource.kind === "ready"} onChanged={onChanged} />
      <TaskRepositoryProgress detail={detail} />
      <div className="space-y-sm">
        <h3 className="font-medium">What counts as done?</h3>
        <p className="text-eyebrow text-muted-foreground">Uses the success criteria saved in setup v{detail.setup_revision}.</p>
        <TaskGoalAdoption workspaceId={workspaceId} fleetId={fleetId} detail={detail} enabled={resource.kind === "ready"} onChanged={onChanged} />
        {detail.context.questions.map(({name, question}, index) => <QuestionDetails key={name} number={index + 1} question={{question: question.instructions, criteria: question.criteria}} />)}
      </div>
      <p className="text-eyebrow text-muted-foreground">{detail.context.usage.leases} of {detail.context.limits.leases} attempts used. {detail.context.checkpoint_id ? "Saved recovery point available." : "No recovery point has been saved."}</p>
    </> : null}
  </div>;
}
