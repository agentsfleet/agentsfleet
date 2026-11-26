"use client";

import type { DurableTaskSummary, DurableTaskDetail as NativeDurableTaskDetail } from "@/lib/api/tasks";
import type { DurableTaskResource } from "./use-task-read";
import { Button } from "@agentsfleet/design-system";
import { durableTaskActivityLabel } from "@/lib/task-activity";
import { workActivity } from "../conversation/summary/WorkActivity";
import { DurableTaskNextStep } from "./TaskNextStep";
import { useFleetDurableTaskView } from "./FleetTaskProvider";
import { DurableTaskControls } from "./TaskControls";
import { DurableTaskRepositoryProgress } from "./TaskRepositoryProgress";
import { DurableTaskStatements } from "./TaskStatements";
import { confirmed } from "./use-task-read";

type Props = {workspaceId: string; fleetId: string; task: DurableTaskSummary; onChanged: () => void};

export function DurableTaskDetailView({workspaceId, fleetId, task, onChanged, resource, retry}: Props & {resource: DurableTaskResource<NativeDurableTaskDetail>; retry: () => void}) {
  const detail = confirmed(resource);
  const view = useFleetDurableTaskView();
  const readOnly = view?.readOnly;
  const latest = view?.turns ? workActivity(view.turns, task.id).items.at(-1) : undefined;
  return <div className="space-y-md pb-md text-body-sm" aria-label={`${task.title} details`}>
    {resource.kind === "loading" ? <output data-chat-loading="true" className="inline-flex items-center gap-xs text-muted-foreground">Loading task details…</output> : null}
    {resource.kind === "failed" ? <div className="space-y-xs text-muted-foreground">
      <output className="block">{detail ? "Couldn’t refresh task details. Last confirmed details are shown." : "Couldn’t load task details."}</output>
      <Button variant="ghost" size="compact" onClick={retry}>Reload details</Button>
    </div> : null}
    {latest ? <p className="text-eyebrow text-muted-foreground">Last activity · {durableTaskActivityLabel(latest)}</p> : null}
    {detail ? <>
      <DurableTaskNextStep detail={detail} />
      {!readOnly ? <DurableTaskControls workspaceId={workspaceId} fleetId={fleetId} detail={detail} enabled={resource.kind === "ready"} onChanged={onChanged} /> : null}
      <DurableTaskRepositoryProgress detail={detail} />
      <DurableTaskStatements workspaceId={workspaceId} fleetId={fleetId} detail={detail} enabled={resource.kind === "ready"}
        readOnly={readOnly === true} onChanged={onChanged} />
      <p className="text-eyebrow text-muted-foreground">{detail.context.usage.runs} of {detail.context.limits.runs} attempts used. {detail.context.checkpoint_id ? "Saved recovery point available." : "No recovery point has been saved."}</p>
    </> : null}
  </div>;
}
