"use client";

import type { ReactNode } from "react";
import type { FleetGrant, FleetTrigger } from "@/lib/api/fleet-detail";
import type { ListResource, Removal } from "./FleetListsProvider";
import { useState } from "react";
import { Alert, Button, ConfirmDialog, IconAction, List, ListItem, Spinner } from "@agentsfleet/design-system";
import { BrainIcon, ClockIcon, KeyIcon, LightningIcon, LockKeyIcon, TrashIcon, XIcon } from "@phosphor-icons/react/ssr";
import { GRANT_STATUS, GITHUB_GRANT, TRIGGER_KIND, githubScope } from "@/lib/api/fleet-detail";
import { SCHEDULE_SYNC, type Schedule } from "@/lib/api/schedule-types";
import { scheduleStateText } from "@/lib/api/schedule-status";
import { WorkSection } from "../shared/summary/WorkSection";
import { ScheduleSync } from "../schedules/ScheduleSync";
import { SUMMARY_ICON, SUMMARY_ROW } from "../shared/summary/summary-row";
import { useFleetLists } from "./FleetListsProvider";

type Row = {key: string; icon: ReactNode; label: string; detail?: string; removal: Removal; removeLabel: string};
const DIALOG: Record<Removal["kind"], {title: string; description: string; confirm: string}> = {
  schedule: {title: "Delete this schedule?", description: "The fleet stops waking on this schedule. Earlier activity stays in the chat.", confirm: "Delete schedule"},
  trigger: {title: "Delete this trigger?", description: "This trigger will stop waking the fleet. Earlier activity stays in the chat.", confirm: "Delete trigger"},
  grant: {title: "Revoke this access?", description: "Work already in progress keeps access. The change applies to future work.", confirm: "Revoke access"},
  memory: {title: "Remove this pinned memory?", description: "Future work will continue without this note.", confirm: "Remove"},
};
const REMOVAL_ICON = {schedule: TrashIcon, trigger: TrashIcon, grant: LockKeyIcon, memory: XIcon};

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
        removal: {kind: "schedule", schedule}, removeLabel: `Delete schedule: ${schedule.description}`}), triggerRow)}
      footer={lists.schedules.kind === "ready" ? <>
        <ScheduleSync compact schedules={lists.schedules.items} busy={retry.busy} onRetry={schedule => {void register(schedule);}} />
        {retry.error ? <Alert variant="destructive" className="mt-md wrap-anywhere px-lg py-md">{retry.error}</Alert> : null}
      </> : null} />
    <ListSection title="Can access" empty="No access approved yet." retry={lists.reload} onRemove={ask}
      resource={rows(lists.grants, grant => grant.status === GRANT_STATUS.approved ? [grantRow(grant)] : [])} />
    <ListSection title="Remembers" empty="Nothing pinned yet. Ask the fleet to remember something." retry={lists.reload} onRemove={ask}
      resource={rows(lists.pinned, memory => [{key: memory.key, icon: <BrainIcon className={SUMMARY_ICON} aria-hidden="true" />, label: memory.content,
        removal: {kind: "memory", memory}, removeLabel: `Remove pinned memory: ${memory.key}`}])} />
    <ConfirmDialog open={removing !== null} onOpenChange={() => setRemoving(null)} title={dialog.title}
      description={dialog.description} confirmLabel={dialog.confirm} intent="destructive" errorMessage={error}
      onConfirm={removing ? () => confirm(removing) : undefined} />
  </>;
}

function ListSection({title, empty, resource, retry, onRemove, footer = null}: {title: string; empty: string; resource: ListResource<Row>; retry: () => void; onRemove: (removal: Removal) => void; footer?: ReactNode}) {
  return <WorkSection title={title}>
    {resource.kind === "loading" ? <Spinner size="sm" label="Loading…" data-chat-loading="true" className="text-eyebrow leading-eyebrow" />
      : resource.kind === "failed" ? <div className="space-y-md">
        <Alert variant="destructive" className="wrap-anywhere px-lg py-md">{resource.message}</Alert>
        <Button variant="outline" size="compact" onClick={retry}>Try again</Button>
      </div>
      : resource.items.length === 0 ? <p className="text-label leading-label text-muted-foreground">{empty}</p>
      : <List variant="plain" aria-label={title} className="space-y-sm text-label leading-label">
        {resource.items.map(row => <ResourceRow key={row.key} row={row} onRemove={onRemove} />)}
      </List>}
    {footer}
  </WorkSection>;
}

function ResourceRow({row, onRemove}: {row: Row; onRemove: (removal: Removal) => void}) {
  const RemoveIcon = REMOVAL_ICON[row.removal.kind];
  return <ListItem className={`${SUMMARY_ROW} min-h-9 items-start gap-sm pointer-coarse:min-h-11`}>
    <span className="flex h-9 shrink-0 items-center pointer-coarse:h-11">{row.icon}</span>
    <span className="min-w-0 flex-1 wrap-anywhere py-md">
      <span className="block font-medium">{row.label}</span>
      {row.detail ? <span className="block text-eyebrow leading-eyebrow text-muted-foreground">{row.detail}</span> : null}
    </span>
    <IconAction variant="ghost" label={row.removeLabel} className="shrink-0" onClick={() => onRemove(row.removal)}>
      <RemoveIcon size={16} aria-hidden="true" />
    </IconAction>
  </ListItem>;
}

function triggerRow(trigger: FleetTrigger): Row {
  const icon = <LightningIcon className={SUMMARY_ICON} aria-hidden="true" />;
  const label = trigger.kind === TRIGGER_KIND.webhook ? `Webhook · ${trigger.source}`
    : trigger.kind === TRIGGER_KIND.appEvent ? `GitHub · ${(trigger.repositories ?? []).join(", ") || "every repository"}`
    : trigger.kind === TRIGGER_KIND.mention ? `Slack · ${trigger.channel ?? "any channel"}` : `${trigger.kind} · ${trigger.source}`;
  const events = trigger.events?.length ? trigger.events.join(", ") : undefined;
  return {key: trigger.id, icon, label, detail: trigger.url ?? events, removal: {kind: "trigger", trigger}, removeLabel: `Delete trigger: ${label}`};
}

function grantRow(grant: FleetGrant): Row {
  const scope = grant.name === GITHUB_GRANT ? githubScope(grant.scope) : null;
  return {key: grant.id, icon: <KeyIcon className={SUMMARY_ICON} aria-hidden="true" />, label: grant.name,
    detail: scope ? `${scope.repositories.join(", ")} · ${scope.access === "write" ? "Read and write" : "Read only"}` : undefined,
    removal: {kind: "grant", grant}, removeLabel: `Revoke access: ${grant.name}`};
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
