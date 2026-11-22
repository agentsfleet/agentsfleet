"use client";

import type { DurableTaskSummary, DurableTaskPage } from "@/lib/api/tasks";
import type { FleetTurn } from "@/lib/streaming/fleet-stream-row";
import type { DurableTaskResource } from "./use-task-read";
import type { DurableTaskDetails } from "./use-task-details";
import type { FleetDurableTaskProps } from "./FleetTaskProvider";
import { useState } from "react";
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger, Button, Section } from "@agentsfleet/design-system";
import { CaretDownIcon, DotOutlineIcon, ClockIcon, WarningIcon } from "@phosphor-icons/react/ssr";
import { durableTaskActivityLabel } from "@/lib/task-activity";
import { DurableTaskDetailView } from "./TaskDetail";
import { accessLost, confirmed } from "./use-task-read";
import { FleetDurableTaskProvider, useFleetDurableTaskView } from "./FleetTaskProvider";
import { workActivity } from "../conversation/summary/WorkActivity";
import { WorkApprovals, WorkChanges, WorkOutputs, WorkSources } from "../conversation/summary/WorkContextSections";
import { WorkSection } from "../shared/summary/WorkSection";

type RowsProps = {workspaceId: string; fleetId: string; tasks: readonly DurableTaskSummary[]; details: DurableTaskDetails; turns: readonly FleetTurn[]; retry: () => void; retryDetails: () => void; expandedDurableTask: string; onExpand: (id: string) => void};

const EMPTY_DURABLE_TASKS: readonly DurableTaskSummary[] = [];
const EMPTY_TURNS: readonly FleetTurn[] = [];

/** Standalone consumers share the same owner; the fleet chat supplies it above both surfaces. */
export function DurableTaskPanel(props: FleetDurableTaskProps) {
  const view = useFleetDurableTaskView();
  return view ? <CurrentWork /> : <FleetDurableTaskProvider {...props}><CurrentWork /></FleetDurableTaskProvider>;
}

function CurrentWork() {
  const view = useFleetDurableTaskView();
  const [workOpen, setWorkOpen] = useState(false);
  const [expandedDurableTask, setExpandedDurableTask] = useState("");
  if (!view?.subject) return null;
  const {workspaceId, fleetId, sources, turns = EMPTY_TURNS, read, approvals} = view;
  const page = read ? confirmed(read.resource) : null;
  const tasks = page?.items ?? EMPTY_DURABLE_TASKS;
  const needsAttention = tasks.some(task => task.status === "blocked" || task.status === "exhausted");
  return <Section asChild className="block gap-0"><section aria-label="Current work" className="ph-no-capture ph-no-record min-w-0 space-y-md">
    {view.readOnly ? <WorkSection title="Status"><p className="text-label font-medium">Archived</p><p className="text-eyebrow text-muted-foreground">Permanently stopped. This fleet cannot resume.</p></WorkSection> : null}
    {!view.readOnly && (!read || !accessLost(read.resource)) ? <WorkApprovals approvals={approvals} scope={{workspaceId, fleetId, subject: view.subject, revision: view.approvalRevision ?? 0}} /> : null}
    {read ? <WorkReadStatus resource={read.resource} retry={read.retry} /> : <WorkLoading />}
    {read && page ? <>
    {!view.readOnly && needsAttention ? <section aria-label="Needs attention" className="space-y-sm">
      <h2 className="text-eyebrow font-medium leading-eyebrow text-warning">Needs attention</h2>
      {tasks.filter(task => task.status === "blocked" || task.status === "exhausted").map(task => <Button key={task.id} variant="ghost" size="sm"
        className="h-auto min-h-11 w-full justify-start whitespace-normal px-0 text-left summary:min-h-9" aria-expanded={workOpen && expandedDurableTask === task.id}
        title="View durable task details" onClick={() => {setWorkOpen(true); setExpandedDurableTask(task.id);}}><WarningIcon size={14} className="shrink-0 text-warning" aria-hidden="true" /><span className="min-w-0 flex-1">{task.title} · {task.status === "blocked" ? "Blocked" : "Attempts exhausted"}</span><CaretDownIcon size={14} className="shrink-0 text-muted-foreground" aria-hidden="true" /></Button>)}
    </section> : null}
    <WorkChanges turns={turns} />
    <WorkOutputs details={read.details} expectedDetails={read.references.length} hideEmpty={turns.length > 0} />
    <WorkSources sources={sources} />
    {tasks.length ? <div className="border-t border-border pt-sm"><Accordion type="single" collapsible value={workOpen ? "durable_tasks" : ""} onValueChange={value => setWorkOpen(value === "durable_tasks")}><AccordionItem value="durable_tasks" className="border-0">
      <AccordionTrigger className="min-h-11 gap-md py-sm text-label font-normal md:min-h-9"><span className="flex flex-1 items-center justify-between gap-md"><span className="inline-flex items-center gap-sm">Tasks{needsAttention ? <WarningIcon size={14} className="text-warning" aria-label="Needs attention" /> : null}</span><span className="text-eyebrow text-muted-foreground">{tasks.length}</span></span></AccordionTrigger>
      <AccordionContent><DurableTaskRows workspaceId={workspaceId} fleetId={fleetId} tasks={tasks} details={read.details} turns={turns} retry={read.retry} retryDetails={read.retryDetails} expandedDurableTask={expandedDurableTask} onExpand={setExpandedDurableTask} /></AccordionContent>
    </AccordionItem></Accordion></div> : null}</> : null}
  </section></Section>;
}

function WorkLoading() {
  return <output data-chat-loading="true" className="inline-flex items-center gap-xs text-eyebrow text-muted-foreground">Reading current work…</output>;
}

function WorkReadStatus({resource, retry}: {resource: DurableTaskResource<DurableTaskPage>; retry: () => void}) {
  if (resource.kind === "loading") return <WorkLoading />;
  if (resource.kind !== "failed") return null;
  return <><output className="block text-eyebrow text-muted-foreground">{confirmed(resource) ? "Connection interrupted. Last confirmed progress is shown. Retry connection to check again." : "Current work is unavailable."}</output><Button variant="ghost" size="sm" onClick={retry}>Retry connection</Button></>;
}

function DurableTaskRows({workspaceId, fleetId, tasks, details, turns, retry, retryDetails, expandedDurableTask, onExpand}: RowsProps) {
  return <Accordion type="single" collapsible value={expandedDurableTask} onValueChange={onExpand}>
    {tasks.map(task => <AccordionItem key={task.id} value={task.id} className="border-0">
      <AccordionTrigger className="min-h-11 gap-sm py-sm text-label font-normal md:min-h-9">
        <DurableTaskIcon task={task} /><span className="min-w-0 flex-1 break-words">{task.title}<DurableTaskAction task={task} details={details} turns={turns} /></span>
      </AccordionTrigger>
      <AccordionContent className="pr-sm"><DurableTaskDetailView workspaceId={workspaceId} fleetId={fleetId} task={task} onChanged={retry}
        resource={details.get(task.id) ?? {kind: "loading", previous: null}} retry={retryDetails} /></AccordionContent>
    </AccordionItem>)}
  </Accordion>;
}

function DurableTaskAction({task, details, turns}: Pick<RowsProps, "details" | "turns"> & {task: DurableTaskSummary}) {
  const resource = details.get(task.id);
  const detail = resource ? confirmed(resource) : null;
  const dependency = detail?.dependency;
  const pending = dependency?.checks.find(check => check.status !== "completed");
  const latest = workActivity(turns, task.id).items.at(-1);
  const action = dependency?.active ? pending
    ? `Waiting for ${pending.run_number ? `CI run #${pending.run_number}` : "recorded checks"} on ${dependency.head.slice(0, 7)}`
    : "Recorded check wait is still active"
    : latest ? durableTaskActivityLabel(latest) : detail ? `Objective: ${detail.context.objective}` : "Reading saved progress…";
  return <span className="line-clamp-2 pt-xs text-eyebrow text-muted-foreground">{action}{resource?.kind === "failed" && detail ? " · last confirmed" : ""}</span>;
}

function DurableTaskIcon({task}: {task: DurableTaskSummary}) {
  if (task.status === "running") return <DotOutlineIcon size={14} className="shrink-0 text-info" aria-label="In progress" />;
  if (task.status === "blocked" || task.status === "exhausted") return <WarningIcon size={14} className="shrink-0 text-warning" aria-label="Needs attention" />;
  return <ClockIcon size={14} className={`shrink-0 ${task.status === "waiting" || task.status === "paused" ? "text-warning" : "text-muted-foreground"}`} aria-label={task.status === "waiting" ? "Waiting" : task.status === "paused" ? "Paused" : "Queued"} />;
}
