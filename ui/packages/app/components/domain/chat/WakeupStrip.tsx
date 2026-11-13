"use client";

import {useState} from "react";
import {ClockIcon, PencilSimpleIcon, PauseIcon, PlayIcon, TrashIcon} from "@phosphor-icons/react/ssr";
import {Button, Collapsible, CollapsibleContent, CollapsibleTrigger, ConfirmDialog} from "@agentsfleet/design-system";
import type {Schedule} from "@/lib/api/draft-types";
import {changeSchedule, deleteSchedule, readSchedule, type ScheduleDetail} from "@/lib/api/schedule-controls";
import {useFleetTaskView} from "./FleetTaskProvider";
import {WakeupState} from "./WorkWakeups";
import {useWakeupControl} from "./use-wakeup-control";
import {WakeupEditor} from "./WakeupEditor";

const EMPTY: readonly Schedule[] = [];
export function WakeupStrip() {
  const view = useFleetTaskView();
  return view?.subject ? <OwnedWakeups key={`${view.subject}:${view.workspaceId}:${view.fleetId}`}
    workspace={view.workspaceId} schedules={view.schedules ?? EMPTY} /> : null;
}

/** The editor belongs to the fleet, so changing which schedule is next cannot discard a draft. */
function OwnedWakeups({workspace, schedules}: {workspace: string; schedules: readonly Schedule[]}) {
  const control = useWakeupControl();
  const [applied, setApplied] = useState(new Map<string, {base: string; value: Schedule | null}>());
  const [editing, setEditing] = useState<ScheduleDetail | null>(null);
  const [deleting, setDeleting] = useState<Schedule | null>(null);
  const updated = (before: Schedule, value: Schedule | null) => {
    const original = schedules.find(row => row.schedule_id === before.schedule_id);
    setApplied(previous => new Map(previous).set(before.schedule_id, {base: JSON.stringify(original), value}));
    setEditing(null); setDeleting(null); control.refresh();
  };
  const visible = schedules.flatMap(original => {
    const saved = applied.get(original.schedule_id);
    const row = saved?.base === JSON.stringify(original) ? saved.value : original;
    return row && row.status !== "removed" && row.status !== "expired" ? [row] : [];
  }).sort((a, b) => Number(b.sync !== "synced") - Number(a.sync !== "synced")
    || (a.next_run_at ? Date.parse(a.next_run_at) : Infinity) - (b.next_run_at ? Date.parse(b.next_run_at) : Infinity));
  const [next, ...rest] = visible;
  const row = (schedule: Schedule) => <WakeupRow key={schedule.schedule_id} schedule={schedule} pending={control.pending}
    onEdit={() => {void control.run(signal => readSchedule(workspace, schedule, signal), setEditing);}} onDelete={() => setDeleting(schedule)}
    onToggle={() => {void control.run(async signal => {
      const current = await readSchedule(workspace, schedule, signal);
      return changeSchedule(workspace, current, {status: schedule.status === "paused" ? "active" : "paused"}, signal);
    }, value => updated(schedule, value));}} />;
  return <>
    {next ? <div className="mx-5 rounded-t-md border border-b-0 border-border bg-muted px-lg py-1.5">
      {row(next)}{rest.length ? <Collapsible><CollapsibleTrigger asChild><Button variant="ghost" size="eyebrow">{rest.length} more wake-ups</Button></CollapsibleTrigger>
        <CollapsibleContent className="max-h-40 overflow-y-auto">{rest.map(row)}</CollapsibleContent></Collapsible> : null}
      {control.error && !editing && !deleting ? <p role="alert" className="text-label text-destructive">{control.error}<Button variant="link" onClick={control.refresh}>Refresh schedule</Button></p> : null}
    </div> : null}
    {editing ? <WakeupEditor schedule={editing} pending={control.pending} error={control.error} onClose={() => setEditing(null)}
      onReload={() => {void control.run(signal => readSchedule(workspace, editing, signal), setEditing);}}
      onSave={change => {void control.run(signal => changeSchedule(workspace, editing, change, signal), value => updated(editing, value));}} /> : null}
    <ConfirmDialog open={deleting !== null} onOpenChange={() => setDeleting(null)} title="Delete wake-up?"
      description="This fleet will stop waking on this schedule. Its previous activity stays in the conversation."
      confirmLabel="Delete wake-up" intent="destructive" errorMessage={control.error}
      onConfirm={deleting ? () => control.run(signal => deleteSchedule(workspace, deleting, signal), value => updated(deleting, value)) : undefined} />
  </>;
}

function WakeupRow({schedule, pending, onEdit, onToggle, onDelete}: {
  schedule: Schedule; pending: boolean; onEdit: () => void; onToggle: () => void; onDelete: () => void;
}) {
  const label = schedule.description || "Scheduled wake-up";
  const locked = pending || !["active", "paused"].includes(schedule.status);
  return <fieldset aria-label={label} className="flex min-w-0 items-center gap-md">
    <ClockIcon size={16} className="shrink-0" aria-hidden="true" />
    <div className="min-w-0 flex-1" title={label}><WakeupState schedule={schedule} compact /></div>
    <Button variant="ghost" size="icon-sm" className="text-foreground" aria-label="Edit wake-up" disabled={locked} onClick={onEdit}><PencilSimpleIcon size={16} /></Button>
    <Button variant="ghost" size="icon-sm" className="text-foreground" aria-label={schedule.status === "paused" ? "Resume wake-up" : "Pause wake-up"} disabled={locked} onClick={onToggle}>{schedule.status === "paused" ? <PlayIcon size={16} /> : <PauseIcon size={16} />}</Button>
    <Button variant="ghost" size="icon-sm" className="text-foreground" aria-label="Delete wake-up" disabled={locked} onClick={onDelete}><TrashIcon size={16} /></Button>
  </fieldset>;
}
