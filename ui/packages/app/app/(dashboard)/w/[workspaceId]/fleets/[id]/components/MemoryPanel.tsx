"use client";

import { useEffect, useId, useRef, useState } from "react";
import {
  Alert,
  Badge,
  Button,
  Card,
  ConfirmDialog,
  DashboardRow,
  DashboardRowGroup,
  EmptyState,
  Label,
  EYEBROW_CLASS,
  List,
  ListItem,
  Switch,
  cn,
  Time,
} from "@agentsfleet/design-system";
import { BrainIcon } from "lucide-react";
import type { MemoryAccess, MemoryEntry } from "@/lib/types";
import { forgetMemoryAction, setMemoryAccessAction } from "../../actions";
import { captureProductEvent } from "@/lib/analytics/posthog";
import { EVENTS } from "@/lib/analytics/events";
import { presentErrorString } from "@/lib/errors";
import {
  MEMORY_EMPTY_DESCRIPTION,
  MEMORY_EMPTY_TITLE,
  MEMORY_FETCH_UNAVAILABLE,
  MEMORY_FORGET_CONFIRM_LABEL,
  MEMORY_FORGET_DIALOG_DESCRIPTION,
  MEMORY_FORGET_DIALOG_TITLE,
  MEMORY_FORGET_LABEL,
  MEMORY_FORGET_MISSING,
  MEMORY_PANEL_TITLE,
  OUTCOME,
} from "./console-copy";

// HTTP 404 — the key was already gone (UZ-MEM-004). The panel surfaces this and
// leaves its list unchanged rather than treating it as a hard failure (§5).
const NOT_FOUND = 404;

export const MEMORY_SHARED_TITLE = "Shared memory";
export const MEMORY_ACCESS_READ_LABEL = "Use shared memory";
export const MEMORY_ACCESS_READ_DESCRIPTION = "See what other fleets in this workspace have shared.";
export const MEMORY_ACCESS_PUBLISH_LABEL = "Share this fleet's memory";
export const MEMORY_ACCESS_PUBLISH_DESCRIPTION = "Let this fleet share what it learns with other fleets in this workspace.";
export const MEMORY_SHARED_LABEL = "shared";
export const MEMORY_SHARED_BY_OTHER = "from another fleet";
const MEMORY_ACCESS_ACTION = "change shared memory access";
const VISIBILITY_WORKSPACE = "workspace";

// Whether `fleetId` wrote `entry`. An older daemon names no writer, and every
// entry it sends is the fleet's own.
function writtenBy(entry: MemoryEntry, fleetId: string): boolean {
  return entry.writer_fleet_id === undefined || entry.writer_fleet_id === fleetId;
}

type Props = {
  workspaceId: string;
  fleetId: string;
  entries: MemoryEntry[] | null;
  /** The fleet's grants; `null` when the daemon sent none. */
  access?: MemoryAccess | null;
  /** Whether the viewer holds `fleet:write`, which the access route takes. */
  canGrant?: boolean;
};

export default function MemoryPanel({ workspaceId, fleetId, entries: initial, access = null, canGrant = false }: Props) {
  const hiddenVersions = useRef(new Map<string, number>());
  const [entries, setEntries] = useState<MemoryEntry[]>(initial ?? []);
  const [pendingEntry, setPendingEntry] = useState<MemoryEntry | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  useEffect(() => {
    if (initial === null) return;
    for (const [key, hiddenAt] of hiddenVersions.current) {
      const current = initial.find((entry) => entry.key === key && writtenBy(entry, fleetId));
      if (!current || current.updated_at > hiddenAt) hiddenVersions.current.delete(key);
    }
    setEntries(initial.filter((entry) => !(writtenBy(entry, fleetId) && hiddenVersions.current.has(entry.key))));
  }, [initial, fleetId]);

  async function forget(entry: MemoryEntry) {
    const key = entry.key;
    setNotice(null);
    const result = await forgetMemoryAction(workspaceId, fleetId, key);
    setPendingEntry(null);
    if (result.ok) {
      hiddenVersions.current.set(key, entry.updated_at);
      setEntries((prev) => prev.filter((e) => e.key !== key || !writtenBy(e, fleetId)));
      captureProductEvent(EVENTS.fleet_memory_forgotten, { fleet_id: fleetId, outcome: OUTCOME.success });
      return;
    }
    captureProductEvent(EVENTS.fleet_memory_forgotten, { fleet_id: fleetId, outcome: OUTCOME.failure });
    // A missing key is not a hard error — the entry is already gone, so say so
    // and leave the list as-is (§5, Failure Modes).
    if (result.status === NOT_FOUND) {
      setNotice(MEMORY_FORGET_MISSING);
      return;
    }
    setNotice(presentErrorString({ errorCode: result.errorCode, message: result.error, action: "forget this memory" }));
  }

  return (
    <Card className="flex flex-col gap-md bg-card p-4" aria-label={MEMORY_PANEL_TITLE}>
      <span className="font-sans text-sm font-medium text-foreground">{MEMORY_PANEL_TITLE}</span>
      {access !== null && canGrant ? (
        <AccessSwitches workspaceId={workspaceId} fleetId={fleetId} initial={access} />
      ) : null}
      {initial === null ? <Alert variant="warning">{MEMORY_FETCH_UNAVAILABLE}</Alert> : null}
      {notice ? <Alert variant="warning">{notice}</Alert> : null}
      {initial === null ? null : entries.length === 0 ? (
        <EmptyState icon={<BrainIcon size={28} />} title={MEMORY_EMPTY_TITLE} description={MEMORY_EMPTY_DESCRIPTION} />
      ) : (
        <List variant="ordered" className="flex list-none flex-col gap-2 space-y-0 pl-0">
          {entries.map((entry) => (
            <ListItem key={`${entry.writer_fleet_id ?? fleetId}:${entry.key}`}>
              <MemoryRow entry={entry} fleetId={fleetId} onForget={() => setPendingEntry(entry)} />
            </ListItem>
          ))}
        </List>
      )}
      <ConfirmDialog
        open={pendingEntry !== null}
        onOpenChange={() => setPendingEntry(null)}
        intent="destructive"
        title={MEMORY_FORGET_DIALOG_TITLE}
        description={MEMORY_FORGET_DIALOG_DESCRIPTION}
        confirmLabel={MEMORY_FORGET_CONFIRM_LABEL}
        onConfirm={pendingEntry ? () => forget(pendingEntry) : undefined}
      />
    </Card>
  );
}

// The two grants as named switches. Each flip sends only the grant it changes,
// and the switch shows the route's answer rather than its own guess.
function AccessSwitches({ workspaceId, fleetId, initial }: { workspaceId: string; fleetId: string; initial: MemoryAccess }) {
  const [access, setAccess] = useState<MemoryAccess>(initial);
  const [saving, setSaving] = useState(false);
  const inFlight = useRef(false);
  const [notice, setNotice] = useState<string | null>(null);

  async function flip(grant: keyof MemoryAccess) {
    // One change at a time: a second flip would send a grant computed from an
    // access the first has not answered yet. The switches stay focusable while
    // saving (aria-disabled), so a keyboard user keeps their place.
    if (inFlight.current) return;
    inFlight.current = true;
    setSaving(true);
    setNotice(null);
    const result = await setMemoryAccessAction(workspaceId, fleetId, { [grant]: !access[grant] });
    inFlight.current = false;
    setSaving(false);
    if (result.ok) {
      setAccess(result.data);
      return;
    }
    setNotice(presentErrorString({ errorCode: result.errorCode, message: result.error, action: MEMORY_ACCESS_ACTION }));
  }

  return (
    <div className="flex flex-col gap-xs">
      <fieldset>
        <legend className={cn("mb-xs text-muted-foreground", EYEBROW_CLASS)}>{MEMORY_SHARED_TITLE}</legend>
        <DashboardRowGroup>
          {ACCESS_GRANTS.map(({ grant, label, description }) => (
            <AccessRow
              key={grant}
              label={label}
              description={description}
              checked={access[grant]}
              busy={saving}
              onFlip={() => void flip(grant)}
            />
          ))}
        </DashboardRowGroup>
      </fieldset>
      {notice ? <Alert variant="warning">{notice}</Alert> : null}
    </div>
  );
}

const ACCESS_GRANTS: { grant: keyof MemoryAccess; label: string; description: string }[] = [
  { grant: "read", label: MEMORY_ACCESS_READ_LABEL, description: MEMORY_ACCESS_READ_DESCRIPTION },
  { grant: "publish", label: MEMORY_ACCESS_PUBLISH_LABEL, description: MEMORY_ACCESS_PUBLISH_DESCRIPTION },
];

type AccessRowProps = { label: string; description: string; checked: boolean; busy: boolean; onFlip: () => void };

// One grant: its name labels the switch and its line describes it.
function AccessRow({ label, description, checked, busy, onFlip }: AccessRowProps) {
  const switchId = useId();
  const descriptionId = useId();
  return (
    <DashboardRow
      titleAs="div"
      className="items-center"
      title={<Label htmlFor={switchId}>{label}</Label>}
      description={<span id={descriptionId}>{description}</span>}
      action={
        <Switch
          id={switchId}
          checked={checked}
          aria-disabled={busy || undefined}
          onCheckedChange={onFlip}
          aria-describedby={descriptionId}
        />
      }
    />
  );
}

function MemoryRow({ entry, fleetId, onForget }: { entry: MemoryEntry; fleetId: string; onForget: () => void }) {
  const shared = entry.visibility === VISIBILITY_WORKSPACE;
  // Another fleet's entry is read here and forgotten only by its writer.
  const othersEntry = !writtenBy(entry, fleetId);
  return (
    <Card className="flex items-start justify-between gap-md p-3">
      <div className="flex min-w-0 flex-col gap-xs">
        <p className="break-words text-sm text-foreground">{entry.content}</p>
        <div className="flex flex-wrap items-center gap-md">
          <Badge variant="default">{entry.category}</Badge>
          {shared ? <Badge variant="cyan">{MEMORY_SHARED_LABEL}</Badge> : null}
          {othersEntry ? <span className="text-sm text-muted-foreground">{MEMORY_SHARED_BY_OTHER}</span> : null}
          <Time
            value={new Date(entry.updated_at)}
            format="relative"
            tooltip={false}
            className="font-mono text-mono leading-mono text-muted-foreground tabular-nums"
          />
        </div>
      </div>
      {othersEntry ? null : (
        <Button type="button" variant="ghost" size="sm" onClick={onForget} aria-label={`${MEMORY_FORGET_LABEL} ${entry.key}`}>
          {MEMORY_FORGET_LABEL}
        </Button>
      )}
    </Card>
  );
}
