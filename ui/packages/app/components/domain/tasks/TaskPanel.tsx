"use client";

import type { DurableTaskDetail, DurableTaskSummary, DurableTaskPage, DurableTaskStatus } from "@/lib/api/tasks";
import type { FleetTurn } from "@/lib/streaming/fleet-stream-row";
import type { Icon } from "@phosphor-icons/react";
import type { DurableTaskResource } from "./use-task-read";
import type { DurableTaskDetails } from "./use-task-details";
import type { FleetDurableTaskProps } from "./FleetTaskProvider";
import { useMemo, useState } from "react";
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger, Button, Section, cn } from "@agentsfleet/design-system";
import { CheckCircleIcon, ClockIcon, DotOutlineIcon, PauseCircleIcon, WarningCircleIcon, XCircleIcon } from "@phosphor-icons/react/ssr";
import { DurableTaskDetailView } from "./TaskDetail";
import { accessLost, confirmed } from "./use-task-read";
import { FleetDurableTaskProvider, useFleetDurableTaskView } from "./FleetTaskProvider";
import { WorkApprovals, WorkChanges, WorkOutputs, WorkSources } from "../conversation/summary/WorkContextSections";
import { RunResources } from "../conversation/summary/RunResources";
import { recordedRunResources } from "../conversation/summary/recorded-run-resources";
import { WorkSection } from "../shared/summary/WorkSection";

type RowsProps = {workspaceId: string; fleetId: string; tasks: readonly DurableTaskSummary[]; details: DurableTaskDetails; retry: () => void; retryDetails: () => void};
type StatusView = {label: string; Icon: Icon; color: string; next?: string; attention?: true};

const EMPTY_DURABLE_TASKS: readonly DurableTaskSummary[] = [];
const EMPTY_TURNS: readonly FleetTurn[] = [];
const TASK_STATUS: Record<DurableTaskStatus, StatusView> = {
  ready: {label: "Queued", Icon: ClockIcon, color: "text-muted-foreground"},
  running: {label: "In progress", Icon: DotOutlineIcon, color: "text-info"},
  waiting: {label: "Waiting", Icon: ClockIcon, color: "text-warning"},
  blocked: {label: "Blocked", Icon: WarningCircleIcon, color: "text-warning", attention: true, next: "Check the conversation for what’s needed."},
  paused: {label: "Paused", Icon: PauseCircleIcon, color: "text-warning"},
  done: {label: "Done", Icon: CheckCircleIcon, color: "text-success"},
  cancelled: {label: "Canceled", Icon: XCircleIcon, color: "text-muted-foreground"},
  exhausted: {label: "Attempts exhausted", Icon: WarningCircleIcon, color: "text-warning", attention: true, next: "Review the last attempt in the conversation."},
};

/** Standalone consumers share the same owner; the fleet chat supplies it above both surfaces. */
export function DurableTaskPanel(props: FleetDurableTaskProps) {
  const view = useFleetDurableTaskView();
  const work = <CurrentWork />;
  return view ? work : <FleetDurableTaskProvider {...props}>{work}</FleetDurableTaskProvider>;
}

function CurrentWork() {
  const view = useFleetDurableTaskView();
  const turns = view?.turns ?? EMPTY_TURNS;
  const resources = useMemo(() => {
    const runs = turns.map(recordedRunResources);
    return {rows: runs.flatMap(run => run.rows), previews: [...new Set(runs.flatMap(run => run.previews))]};
  }, [turns]);
  if (!view?.subject) return null;
  const {workspaceId, fleetId, sources, files, read, approvals} = view;
  const page = read ? confirmed(read.resource) : null;
  const tasks = page?.items ?? EMPTY_DURABLE_TASKS;
  return <Section asChild className="block gap-0"><section aria-label="Current work" className="ph-no-capture ph-no-record min-w-0">
    {view.readOnly ? <WorkSection title="Status"><p className="text-label font-medium">Archived</p><p className="text-eyebrow text-muted-foreground">Permanently stopped. This fleet cannot resume.</p></WorkSection> : null}
    {!view.readOnly && (!read || !accessLost(read.resource)) ? <WorkApprovals approvals={approvals} scope={{workspaceId, fleetId, subject: view.subject, revision: view.approvalRevision ?? 0}} /> : null}
    {read ? <WorkReadStatus resource={read.resource} retry={read.retry} /> : <WorkLoading />}
    {read && page ? <>
    {tasks.length ? <WorkSection title="Tasks"><DurableTaskRows workspaceId={workspaceId} fleetId={fleetId} tasks={tasks}
      details={read.details} retry={read.retry} retryDetails={read.retryDetails} /></WorkSection> : null}
    <WorkChanges turns={turns} repositories={view.changeRepositories} />
    <WorkOutputs previews={resources.previews} details={read.details} expectedDetails={read.references.length} hideEmpty={turns.length > 0} />
    <RunResources rows={resources.rows} />
    <WorkSources sources={sources} files={files} />
    </> : null}
  </section></Section>;
}

function WorkLoading() {
  return <output data-chat-loading="true" className="inline-flex items-center gap-xs text-eyebrow text-muted-foreground not-last:mb-md">Reading current work…</output>;
}

function WorkReadStatus({resource, retry}: {resource: DurableTaskResource<DurableTaskPage>; retry: () => void}) {
  if (resource.kind === "loading") return <WorkLoading />;
  if (resource.kind !== "failed") return null;
  return <><output className="mb-md block text-eyebrow text-muted-foreground">{confirmed(resource) ? "Couldn’t refresh current work. Last confirmed progress is shown." : "Current work is unavailable."}</output><Button variant="ghost" size="compact" className="not-last:mb-md" onClick={retry}>Reload current work</Button></>;
}

function DurableTaskRows({workspaceId, fleetId, tasks, details, retry, retryDetails}: RowsProps) {
  const ordered = tasks.toSorted((a, b) => Number(Boolean(TASK_STATUS[b.status].attention)) - Number(Boolean(TASK_STATUS[a.status].attention)));
  const first = ordered[0]?.id;
  const [selected, setSelected] = useState(first);
  const openTask = selected && !ordered.some(task => task.id === selected) ? first : selected;
  if (openTask !== selected) setSelected(openTask);
  return <Accordion type="single" collapsible value={openTask} onValueChange={setSelected} className="space-y-md">
    {ordered.map(task => {
      const status = TASK_STATUS[task.status];
      const resource = details.get(task.id) ?? {kind: "loading", previous: null} as const;
      const next = dependencySummary(resource, task.resource_version) ?? status.next;
      return <AccordionItem key={task.id} value={task.id} className="rounded-md border border-border">
        <AccordionTrigger className="min-h-11 min-w-0 items-start gap-md p-lg text-description leading-normal hover:bg-muted focus-visible:ring-inset focus-visible:ring-offset-0 summary:min-h-9 [&>svg:last-child]:ml-0 [&>svg]:mt-xs">
          <span className="min-w-0 flex-1 wrap-anywhere">
            <span className="block font-semibold">{task.title}</span>
            <span className={cn("mt-xs flex items-center gap-xs text-eyebrow leading-eyebrow", status.color)}>
              <status.Icon size={14} className="shrink-0" aria-hidden="true" />{status.label}
            </span>
            {next ? <span className="mt-md block text-label leading-label text-muted-foreground">{next}</span> : null}
          </span>
        </AccordionTrigger>
        <AccordionContent className="mx-lg border-t border-border-subtle pt-lg pb-lg"><DurableTaskDetailView workspaceId={workspaceId} fleetId={fleetId}
          task={task} onChanged={retry} resource={resource} retry={retryDetails} /></AccordionContent>
      </AccordionItem>;
    })}
  </Accordion>;
}

function dependencySummary(resource: DurableTaskResource<DurableTaskDetail>, version: number): string | undefined {
  const detail = confirmed(resource);
  if (!detail?.dependency?.active || detail.resource_version !== version) return undefined;
  const dependency = detail.dependency;
  const pending = dependency.checks.find(check => check.status !== "completed");
  const wait = pending ? `Waiting for ${pending.run_number ? `CI run #${pending.run_number}` : "recorded checks"} on ${dependency.head.slice(0, 7)}.`
    : "Waiting for the fleet to confirm the checks.";
  return resource.kind === "ready" ? wait : `Last confirmed: ${wait}`;
}
