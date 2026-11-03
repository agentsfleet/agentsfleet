"use client";

import { useCallback } from "react";
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger, Button, SignalSpinner } from "@agentsfleet/design-system";
import { readTasks, type TaskStatus, type TaskSummary } from "@/lib/api/tasks";
import { TaskDetail } from "./TaskDetail";
import { confirmed, useTaskRead } from "./use-task-read";

type Props = {workspaceId: string; fleetId: string; subject: string | null; revision: number};
const LABELS: Record<TaskStatus, string> = {
  ready: "Queued", running: "In progress", waiting: "Waiting", blocked: "Needs attention",
  paused: "Paused", done: "Completed", cancelled: "Cancelled", exhausted: "Limit reached",
};

export function TaskPanel(props: Props) {
  if (!props.subject) return null;
  return <CurrentWork key={`${props.subject}:${props.workspaceId}:${props.fleetId}`} {...props} />;
}

function CurrentWork({workspaceId, fleetId, revision}: Props) {
  const load = useCallback((signal: AbortSignal) => readTasks(workspaceId, fleetId, signal), [workspaceId, fleetId]);
  const {resource, retry} = useTaskRead(load, revision);
  const page = confirmed(resource);
  if (resource.kind === "ready" && resource.value.items.length === 0) return null;
  return <section aria-label="Current work" className="mx-auto mb-sm min-h-0 max-h-[min(24rem,35dvh)] w-full max-w-chat overflow-y-auto overscroll-contain rounded-lg border border-border px-lg pt-md">
    <div className="flex items-center justify-between gap-sm">
      <h2 className="text-label font-medium">Current work{page ? ` · ${page.items.length}` : ""}</h2>
      <Button variant="ghost" size="sm" onClick={retry} aria-label="Refresh current work">Refresh</Button>
    </div>
    {resource.kind === "loading" ? <output className="inline-flex items-center gap-xs pb-sm text-body-sm text-muted-foreground"><SignalSpinner aria-hidden="true" />Reading current work…</output> : null}
    {resource.kind === "failed" ? <output className="block pb-sm text-eyebrow text-muted-foreground">{page ? "Couldn’t refresh. Last confirmed progress is shown." : "Current work is unavailable. Refresh to try again."}</output> : null}
    {page ? <>
      <p className="pb-sm text-eyebrow text-muted-foreground">{overview(page.items)}</p>
      <Accordion type="single" collapsible>
        {page.items.map(task => <AccordionItem key={task.id} value={task.id} className="border-t border-border">
          <AccordionTrigger className="min-h-11 gap-sm py-sm text-label font-normal md:min-h-9">
            <span className="min-w-0 flex-1 break-words">{task.title}</span>
            <span className={task.status === "running" ? "shrink-0 text-info" : "shrink-0 text-muted-foreground"}>{LABELS[task.status]}</span>
          </AccordionTrigger>
          <AccordionContent className="pr-sm"><TaskDetail workspaceId={workspaceId} fleetId={fleetId} task={task} /></AccordionContent>
        </AccordionItem>)}
      </Accordion>
      <p className="py-sm text-eyebrow text-muted-foreground">Expand work for its saved criteria.</p>
    </> : null}
  </section>;
}

function overview(tasks: readonly TaskSummary[]): string {
  const counts = new Map<TaskStatus, number>();
  for (const task of tasks) counts.set(task.status, (counts.get(task.status) ?? 0) + 1);
  return [...counts].map(([state, count]) => `${count} ${LABELS[state].toLowerCase()}`).join(" · ");
}
