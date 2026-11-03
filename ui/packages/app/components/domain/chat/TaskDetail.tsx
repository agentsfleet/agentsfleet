"use client";

import { useCallback } from "react";
import { Button, SignalSpinner } from "@agentsfleet/design-system";
import { readTask, type TaskSummary } from "@/lib/api/tasks";
import { QuestionDetails } from "./SetupQuestions";
import { confirmed, useTaskRead } from "./use-task-read";

type Props = {workspaceId: string; fleetId: string; task: TaskSummary};

export function TaskDetail({workspaceId, fleetId, task}: Props) {
  const load = useCallback((signal: AbortSignal) => readTask(workspaceId, fleetId, task.id, signal), [workspaceId, fleetId, task.id]);
  const {resource, retry} = useTaskRead(load, task.resource_version);
  const detail = confirmed(resource);
  return <div className="space-y-md pb-md text-body-sm" aria-label={`${task.title} details`}>
    {resource.kind === "loading" ? <output className="inline-flex items-center gap-xs text-muted-foreground"><SignalSpinner aria-hidden="true" />Reading saved progress…</output> : null}
    {resource.kind === "failed" ? <div className="flex items-center justify-between gap-sm text-muted-foreground">
      <p>{detail ? "Progress couldn’t be refreshed. Last confirmed details are shown." : "Saved progress is unavailable."}</p>
      <Button variant="ghost" size="sm" onClick={retry}>Retry details</Button>
    </div> : null}
    {detail ? <>
      <p className="whitespace-pre-wrap break-words">{detail.context.objective}</p>
      <div className="space-y-sm">
        <h3 className="font-medium">What counts as done?</h3>
        <p className="text-eyebrow text-muted-foreground">Uses the success criteria saved in setup v{detail.setup_revision}.</p>
        {detail.context.questions.map(({name, question}, index) => <QuestionDetails key={name} number={index + 1} question={{question: question.instructions, criteria: question.criteria}} />)}
      </div>
      <p className="text-eyebrow text-muted-foreground">{detail.context.usage.leases} of {detail.context.limits.leases} attempts used. {detail.context.checkpoint_id ? "Saved recovery point available." : "No recovery point has been saved."}</p>
    </> : null}
  </div>;
}
