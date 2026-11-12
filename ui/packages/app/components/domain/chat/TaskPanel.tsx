"use client";

import type { TaskSummary, TaskPage } from "@/lib/api/tasks";
import type { FleetEvent } from "@/lib/streaming/fleet-stream-row";
import type { TaskResource } from "./use-task-read";
import type { TaskDetails } from "./use-task-details";
import type { FleetTaskProps } from "./FleetTaskProvider";
import { useState } from "react";
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger, Button, Section } from "@agentsfleet/design-system";
import { CaretDownIcon, DotOutlineIcon, ClockIcon, WarningIcon } from "@phosphor-icons/react/ssr";
import { taskActivityLabel } from "@/lib/task-activity";
import { TaskDetailView } from "./TaskDetail";
import { accessLost, confirmed } from "./use-task-read";
import { FleetTaskProvider, useFleetTaskView } from "./FleetTaskProvider";
import { workActivity } from "./WorkActivity";
import { WorkApprovals, WorkChanges, WorkOutputs, WorkSources } from "./WorkContextSections";
import { WorkWakeups } from "./WorkWakeups";

type RowsProps = {workspaceId: string; fleetId: string; tasks: readonly TaskSummary[]; details: TaskDetails; events: readonly FleetEvent[]; retry: () => void; retryDetails: () => void; expandedTask: string; onExpand: (id: string) => void};

const EMPTY_TASKS: readonly TaskSummary[] = [];
const EMPTY_EVENTS: readonly FleetEvent[] = [];

/** Standalone consumers share the same owner; the fleet chat supplies it above both surfaces. */
export function TaskPanel(props: FleetTaskProps) {
  const view = useFleetTaskView();
  return view ? <CurrentWork /> : <FleetTaskProvider {...props}><CurrentWork /></FleetTaskProvider>;
}

function CurrentWork() {
  const view = useFleetTaskView();
  const [workOpen, setWorkOpen] = useState(false);
  const [expandedTask, setExpandedTask] = useState("");
  if (!view?.subject) return null;
  const {workspaceId, fleetId, sources, events = EMPTY_EVENTS, read, approvals, schedules} = view;
  const page = read ? confirmed(read.resource) : null;
  const tasks = page?.items ?? EMPTY_TASKS;
  const needsAttention = tasks.some(task => task.status === "blocked" || task.status === "exhausted");
  return <Section asChild className="block gap-0"><section aria-label="Current work" className="ph-no-capture ph-no-record min-w-0 space-y-md">
    {!read || !accessLost(read.resource) ? <WorkApprovals approvals={approvals} scope={{workspaceId, fleetId, subject: view.subject, revision: view.approvalRevision ?? 0}} /> : null}
    {read ? <WorkReadStatus resource={read.resource} retry={read.retry} /> : <WorkLoading />}
    {read && page ? <>
    {needsAttention ? <section aria-label="Needs attention" className="space-y-sm">
      <h2 className="text-eyebrow font-medium leading-eyebrow text-warning">Needs attention</h2>
      {tasks.filter(task => task.status === "blocked" || task.status === "exhausted").map(task => <Button key={task.id} variant="ghost" size="sm"
        className="h-auto min-h-11 w-full justify-start whitespace-normal px-0 text-left summary:min-h-9" aria-expanded={workOpen && expandedTask === task.id}
        title="View task details" onClick={() => {setWorkOpen(true); setExpandedTask(task.id);}}><WarningIcon size={14} className="shrink-0 text-warning" aria-hidden="true" /><span className="min-w-0 flex-1">{task.title} · {task.status === "blocked" ? "Blocked" : "Attempts exhausted"}</span><CaretDownIcon size={14} className="shrink-0 text-muted-foreground" aria-hidden="true" /></Button>)}
    </section> : null}
    <WorkWakeups schedules={schedules} compact />
    <WorkChanges events={events} />
    <WorkOutputs details={read.details} expectedDetails={read.references.length} hideEmpty={events.length > 0} />
    <WorkSources sources={sources} />
    {tasks.length ? <div className="border-t border-border pt-sm"><Accordion type="single" collapsible value={workOpen ? "tasks" : ""} onValueChange={value => setWorkOpen(value === "tasks")}><AccordionItem value="tasks" className="border-0">
      <AccordionTrigger className="min-h-11 gap-md py-sm text-label font-normal md:min-h-9"><span className="flex flex-1 items-center justify-between gap-md"><span className="inline-flex items-center gap-sm">Tasks{needsAttention ? <WarningIcon size={14} className="text-warning" aria-label="Needs attention" /> : null}</span><span className="text-eyebrow text-muted-foreground">{tasks.length}</span></span></AccordionTrigger>
      <AccordionContent><TaskRows workspaceId={workspaceId} fleetId={fleetId} tasks={tasks} details={read.details} events={events} retry={read.retry} retryDetails={read.retryDetails} expandedTask={expandedTask} onExpand={setExpandedTask} /></AccordionContent>
    </AccordionItem></Accordion></div> : null}</> : null}
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

function TaskRows({workspaceId, fleetId, tasks, details, events, retry, retryDetails, expandedTask, onExpand}: RowsProps) {
  return <Accordion type="single" collapsible value={expandedTask} onValueChange={onExpand}>
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
  if (task.status === "running") return <DotOutlineIcon size={14} className="shrink-0 text-info" aria-label="In progress" />;
  if (task.status === "blocked" || task.status === "exhausted") return <WarningIcon size={14} className="shrink-0 text-warning" aria-label="Needs attention" />;
  return <ClockIcon size={14} className={`shrink-0 ${task.status === "waiting" || task.status === "paused" ? "text-warning" : "text-muted-foreground"}`} aria-label={task.status === "waiting" ? "Waiting" : task.status === "paused" ? "Paused" : "Queued"} />;
}
