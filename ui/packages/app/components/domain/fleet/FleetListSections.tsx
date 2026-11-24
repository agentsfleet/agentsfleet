"use client";

import type { ReactNode } from "react";
import type { FleetGrant, FleetTrigger } from "@/lib/api/fleet-detail";
import type { ListResource, Removal } from "./FleetListsProvider";
import { useState } from "react";
import { Button, ConfirmDialog, IconAction, List, ListItem } from "@agentsfleet/design-system";
import { BrainIcon, ClockIcon, KeyIcon, LightningIcon, TrashIcon } from "@phosphor-icons/react/ssr";
import { GRANT_STATUS, GITHUB_GRANT, TRIGGER_KIND, githubScope } from "@/lib/api/fleet-detail";
import { SCHEDULE_SYNC, type Schedule } from "@/lib/api/schedule-types";
import { scheduleStateText } from "@/lib/api/schedule-status";
import { WorkSection } from "../shared/summary/WorkSection";
import { ScheduleSync } from "../schedules/ScheduleSync";
import { SUMMARY_ICON, SUMMARY_ROW } from "../shared/summary/summary-row";
import { useFleetLists } from "./FleetListsProvider";

type Row = {key: string; icon: ReactNode; label: string; detail?: string; removal: Removal; removeLabel: string};
const DIALOG: Record<Removal["kind"], {title: string; description: string; confirm: string}> = {
  schedule: {title: "Remove this wake-up?", description: "The fleet stops waking on this schedule. Its earlier runs stay in the chat.", confirm: "Remove wake-up"},
  trigger: {title: "Remove this trigger?", description: "Deliveries to it stop reaching the fleet. Its earlier runs stay in the chat.", confirm: "Remove trigger"},
  grant: {title: "Remove this access?", description: "The fleet's next run no longer holds it. A run already working keeps what it was issued.", confirm: "Remove access"},
  memory: {title: "Forget this pinned memory?", description: "Later runs no longer see it at the start of their prompt.", confirm: "Forget"},
};

/** "Wakes on", "Can access" and "Remembers": what the fleet set up or was given, each row removable by a person. */
export function FleetListSections() {
  const lists = useFleetLists();
  const [removing, setRemoving] = useState<Removal | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [retry, setRetry] = useState<{busy: boolean; error: string | null}>({busy: false, error: null});
  if (!lists) return null;
  const register = async (schedule: Schedule): Promise<void> => {
    setRetry({busy: true, error: null});
    setRetry({busy: false, error: await lists.retrySchedule(schedule)});
  };
  const confirm = async (removal: Removal): Promise<void> => {
    const refusal = await lists.remove(removal);
    if (refusal) {setError(refusal); return;}
    setRemoving(null);
  };
  const ask = (removal: Removal) => {setError(null); setRemoving(removal);};
  const dialog = removing ? DIALOG[removing.kind] : DIALOG.memory;
  return <>
    <ListSection title="Wakes on" empty="Only a message wakes this fleet." retry={lists.reload} onRemove={ask}
      resource={merge(lists.schedules, lists.triggers, schedule => ({key: schedule.schedule_id, icon: <ClockIcon className={SUMMARY_ICON} aria-hidden="true" />,
        label: schedule.description, detail: schedule.status === "active" && schedule.sync === SCHEDULE_SYNC.synced ? undefined : scheduleStateText(schedule),
        removal: {kind: "schedule", schedule}, removeLabel: `Remove wake-up: ${schedule.description}`}), triggerRow)}
      footer={lists.schedules.kind === "ready" ? <>
        <ScheduleSync compact schedules={lists.schedules.items} busy={retry.busy} onRetry={schedule => {void register(schedule);}} />
        {retry.error ? <p role="alert" className="text-eyebrow text-destructive">{retry.error}</p> : null}
      </> : null} />
    <ListSection title="Can access" empty="Nothing yet. The fleet asks when it needs access." retry={lists.reload} onRemove={ask}
      resource={rows(lists.grants, grant => grant.status === GRANT_STATUS.approved ? [grantRow(grant)] : [])} />
    <ListSection title="Remembers" empty="Nothing pinned. Ask the fleet to remember standing guidance." retry={lists.reload} onRemove={ask}
      resource={rows(lists.pinned, memory => [{key: memory.key, icon: <BrainIcon className={SUMMARY_ICON} aria-hidden="true" />, label: memory.content,
        removal: {kind: "memory", memory}, removeLabel: `Forget pinned memory: ${memory.key}`}])} />
    <ConfirmDialog open={removing !== null} onOpenChange={() => setRemoving(null)} title={dialog.title}
      description={dialog.description} confirmLabel={dialog.confirm} intent="destructive" errorMessage={error}
      onConfirm={removing ? () => confirm(removing) : undefined} />
  </>;
}

function ListSection({title, empty, resource, retry, onRemove, footer = null}: {title: string; empty: string; resource: ListResource<Row>; retry: () => void; onRemove: (removal: Removal) => void; footer?: ReactNode}) {
  return <WorkSection title={title}>
    {resource.kind === "loading" ? <output data-chat-loading="true" className="text-eyebrow text-muted-foreground">Loading…</output>
      : resource.kind === "failed" ? <div className="space-y-sm"><p className="text-eyebrow text-muted-foreground">{resource.message}</p>
        <Button variant="ghost" size="sm" onClick={retry}>Try again</Button></div>
      : resource.items.length === 0 ? <p className="text-eyebrow leading-eyebrow text-muted-foreground">{empty}</p>
      : <List variant="plain" aria-label={title} className="space-y-sm text-label leading-label">{resource.items.map(row => <ListItem key={row.key} className={`${SUMMARY_ROW} group min-h-9`}>
        {row.icon}
        <span className="min-w-0 flex-1 break-words"><span className="block">{row.label}</span>
          {row.detail ? <span className="block break-all text-eyebrow text-text-dim">{row.detail}</span> : null}</span>
        <IconAction variant="ghost" label={row.removeLabel} className="shrink-0" onClick={() => onRemove(row.removal)}>
          <TrashIcon size={14} aria-hidden="true" />
        </IconAction>
      </ListItem>)}</List>}
    {footer}
  </WorkSection>;
}

function triggerRow(trigger: FleetTrigger): Row {
  const icon = <LightningIcon className={SUMMARY_ICON} aria-hidden="true" />;
  const label = trigger.kind === TRIGGER_KIND.webhook ? `Webhook · ${trigger.source}`
    : trigger.kind === TRIGGER_KIND.appEvent ? `GitHub · ${(trigger.repositories ?? []).join(", ") || "every repository"}`
    : trigger.kind === TRIGGER_KIND.mention ? `Slack · ${trigger.channel ?? "any channel"}` : `${trigger.kind} · ${trigger.source}`;
  const events = trigger.events?.length ? trigger.events.join(", ") : undefined;
  return {key: trigger.id, icon, label, detail: trigger.url ?? events, removal: {kind: "trigger", trigger}, removeLabel: `Remove trigger: ${label}`};
}

function grantRow(grant: FleetGrant): Row {
  const scope = grant.name === GITHUB_GRANT ? githubScope(grant.scope) : null;
  return {key: grant.id, icon: <KeyIcon className={SUMMARY_ICON} aria-hidden="true" />, label: grant.name,
    detail: scope ? `${scope.repositories.join(", ")} · ${scope.access === "write" ? "Read and write" : "Read only"}` : undefined,
    removal: {kind: "grant", grant}, removeLabel: `Remove access: ${grant.name}`};
}

function rows<T>(resource: ListResource<T>, toRows: (item: T) => Row[]): ListResource<Row> {
  return resource.kind === "ready" ? {kind: "ready", items: resource.items.flatMap(toRows)} : resource;
}

// Schedules and triggers are one list to a person: either fails, and the list is not whole.
function merge<A, B>(left: ListResource<A>, right: ListResource<B>, leftRow: (item: A) => Row, rightRow: (item: B) => Row): ListResource<Row> {
  if (left.kind === "failed") return left;
  if (right.kind === "failed") return right;
  if (left.kind === "loading" || right.kind === "loading") return {kind: "loading"};
  return {kind: "ready", items: [...left.items.map(leftRow), ...right.items.map(rightRow)]};
}
