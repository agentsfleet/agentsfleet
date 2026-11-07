"use client";

import type { TaskSummary, TaskPage } from "@/lib/api/tasks";
import type { FleetEvent } from "@/lib/streaming/fleet-stream-row";
import type { TaskResource } from "./use-task-read";
import type { TaskDetails } from "./use-task-details";
import type { FleetTaskProps } from "./FleetTaskProvider";
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger, Button, Section } from "@agentsfleet/design-system";
import { CircleDot, Clock3, TriangleAlert } from "lucide-react";
import { taskActivityLabel } from "@/lib/task-activity";
import { TaskDetailView } from "./TaskDetail";
import { confirmed } from "./use-task-read";
import { FleetTaskProvider, useFleetTaskView } from "./FleetTaskProvider";
import { workActivity } from "./WorkActivity";
import { WorkChanges, WorkOutputs, WorkSection, WorkSources } from "./WorkContextSections";

type RowsProps = {workspaceId: string; fleetId: string; tasks: readonly TaskSummary[]; details: TaskDetails; events: readonly FleetEvent[]; retry: () => void; retryDetails: () => void};

const EMPTY_TASKS: readonly TaskSummary[] = [];
const EMPTY_EVENTS: readonly FleetEvent[] = [];

/** Standalone consumers share the same owner; the fleet chat supplies it above both surfaces. */
export function TaskPanel(props: FleetTaskProps) {
  const view = useFleetTaskView();
  return view ? <CurrentWork /> : <FleetTaskProvider {...props}><CurrentWork /></FleetTaskProvider>;
}

function CurrentWork() {
  const view = useFleetTaskView();
  if (!view?.subject) return null;
  const {workspaceId, fleetId, sources, events = EMPTY_EVENTS, read} = view;
  if (!read) return <Section asChild className="block gap-0"><section aria-label="Current work"><WorkLoading /></section></Section>;
  const {resource, details, retry, retryDetails} = read;
  const page = confirmed(resource);
  const tasks = page?.items ?? EMPTY_TASKS;
  return <Section asChild className="block gap-0"><section aria-label="Current work" className="ph-no-capture ph-no-record min-w-0 space-y-lg">
    <WorkReadStatus resource={resource} retry={retry} />
    {page ? <><WorkSection title="Durable tasks">
      {!tasks.length ? <p className="text-eyebrow text-muted-foreground">No unfinished tasks.</p> : <TaskRows workspaceId={workspaceId} fleetId={fleetId} tasks={tasks} details={details} events={events} retry={retry} retryDetails={retryDetails} />}
    </WorkSection><WorkOutputs details={details} /><WorkChanges events={events} /><WorkSources sources={sources} /></> : null}
  </section></Section>;
}

function WorkLoading() {
  return <output data-chat-loading="true" className="inline-flex items-center gap-xs text-eyebrow text-muted-foreground">Reading current work…</output>;
}

function WorkReadStatus({resource, retry}: {resource: TaskResource<TaskPage>; retry: () => void}) {
  if (resource.kind === "loading") return <WorkLoading />;
  if (resource.kind !== "failed") return null;
  return <><output className="block text-eyebrow text-muted-foreground">{confirmed(resource) ? "Connection interrupted. Last confirmed progress is shown. Retry connection to check again." : "Current work is unavailable."}</output><Button variant="ghost" size="sm" onClick={retry}>Retry connection</Button></>;
}

function TaskRows({workspaceId, fleetId, tasks, details, events, retry, retryDetails}: RowsProps) {
  return <Accordion type="single" collapsible>
    {tasks.map(task => <AccordionItem key={task.id} value={task.id} className="border-0">
      <AccordionTrigger className="min-h-11 gap-sm py-sm text-label font-normal md:min-h-9">
        <TaskIcon task={task} /><span className="min-w-0 flex-1 break-words">{task.title}<TaskAction task={task} details={details} events={events} /></span>
      </AccordionTrigger>
      <AccordionContent className="pr-sm"><TaskDetailView workspaceId={workspaceId} fleetId={fleetId} task={task} onChanged={retry}
        resource={details.get(task.id) ?? {kind: "loading", previous: null}} retry={retryDetails} /></AccordionContent>
    </AccordionItem>)}
  </Accordion>;
}

function TaskAction({task, details, events}: Pick<RowsProps, "details" | "events"> & {task: TaskSummary}) {
  const resource = details.get(task.id);
  const detail = resource ? confirmed(resource) : null;
  const dependency = detail?.dependency;
  const pending = dependency?.checks.find(check => check.status !== "completed");
  const latest = workActivity(events, task.id).items.at(-1);
  const action = dependency?.active ? pending
    ? `Waiting for ${pending.run_number ? `CI run #${pending.run_number}` : "recorded checks"} on ${dependency.head.slice(0, 7)}`
    : "Recorded check wait is still active"
    : latest ? taskActivityLabel(latest) : detail ? `Objective: ${detail.context.objective}` : "Reading saved progress…";
  return <span className="line-clamp-2 pt-xs text-eyebrow text-muted-foreground">{action}{resource?.kind === "failed" && detail ? " · last confirmed" : ""}</span>;
}

function TaskIcon({task}: {task: TaskSummary}) {
  if (task.status === "running") return <CircleDot size={14} className="shrink-0 text-info" aria-label="In progress" />;
  if (task.status === "blocked" || task.status === "exhausted") return <TriangleAlert size={14} className="shrink-0 text-warning" aria-label="Needs attention" />;
  return <Clock3 size={14} className={`shrink-0 ${task.status === "waiting" || task.status === "paused" ? "text-warning" : "text-muted-foreground"}`} aria-label={task.status === "waiting" ? "Waiting" : task.status === "paused" ? "Paused" : "Queued"} />;
}
