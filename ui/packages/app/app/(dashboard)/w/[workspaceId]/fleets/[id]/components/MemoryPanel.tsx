"use client";

import { useEffect, useRef, useState } from "react";
import {
  Alert,
  Button,
  ConfirmDialog,
  EmptyState,
  List,
  ListItem,
  Section,
  Time,
} from "@agentsfleet/design-system";
import { FileTextIcon } from "@phosphor-icons/react/ssr";
import { useFleetLists } from "@/components/domain/fleet/FleetListsProvider";
import { byInstant } from "@/lib/api/decode";
import type { MemoryEntry } from "@/lib/types";
import { forgetMemoryAction } from "../../actions";
import { captureProductEvent } from "@/lib/analytics/posthog";
import { EVENTS } from "@/lib/analytics/events";
import { presentErrorString } from "@/lib/errors";
import {
  MEMORY_EMPTY_DESCRIPTION,
  MEMORY_EMPTY_TITLE,
  MEMORY_FORGET_CONFIRM_LABEL,
  MEMORY_FORGET_DIALOG_TITLE,
  MEMORY_FORGET_LABEL,
  MEMORY_FORGET_MISSING,
  OUTCOME,
} from "./console-copy";

// HTTP 404 — the key was already gone (UZ-MEM-004). The panel surfaces this and
// leaves its list unchanged rather than treating it as a hard failure (§5).
const NOT_FOUND = 404;

const VISIBILITY_WORKSPACE = "workspace";

function writtenBy(entry: MemoryEntry, fleetId: string): boolean {
  return entry.writer_fleet_id === fleetId;
}

type Props = {
  workspaceId: string;
  fleetId: string;
  entries: MemoryEntry[];
};

export default function MemoryPanel({ workspaceId, fleetId, entries: initial }: Props) {
  const hiddenVersions = useRef(new Map<string, string>());
  const [entries, setEntries] = useState(initial);
  const [pendingEntry, setPendingEntry] = useState<MemoryEntry | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const reloadLists = useFleetLists()?.reload;

  useEffect(() => {
    for (const [key, hiddenAt] of hiddenVersions.current) {
      const current = initial.find((entry) => entry.key === key && writtenBy(entry, fleetId));
      if (!current || byInstant(current.updated_at, hiddenAt) > 0) hiddenVersions.current.delete(key);
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
      reloadLists?.();
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
    <Section className="gap-xl">
      {notice ? <Alert variant="warning">{notice}</Alert> : null}
      {entries.length === 0 ? <EmptyState icon={<FileTextIcon size={20} />} title={MEMORY_EMPTY_TITLE} description={MEMORY_EMPTY_DESCRIPTION}
        className="border-0 bg-background" /> : <>
        <MemoryGroup title="Saved by this fleet" entries={entries.filter(entry => writtenBy(entry, fleetId))} forget={setPendingEntry} />
        <MemoryGroup title="Shared by other fleets" entries={entries.filter(entry => !writtenBy(entry, fleetId))} />
      </>}
      <ConfirmDialog
        open={pendingEntry !== null}
        onOpenChange={() => setPendingEntry(null)}
        intent="destructive"
        title={MEMORY_FORGET_DIALOG_TITLE}
        description={pendingEntry ? <>
          <span>{pendingEntry.visibility === VISIBILITY_WORKSPACE ? "Fleets in this workspace will lose access to this memory." : "This fleet will stop using this memory."}</span>
          <span className="mt-lg block wrap-anywhere rounded-control bg-background p-lg text-foreground">{pendingEntry.content}</span>
        </> : undefined}
        confirmLabel={MEMORY_FORGET_CONFIRM_LABEL}
        onConfirm={pendingEntry ? () => forget(pendingEntry) : undefined}
      />
    </Section>
  );
}

function MemoryGroup({title, entries, forget}: {title: string; entries: MemoryEntry[]; forget?: (entry: MemoryEntry) => void}) {
  if (!entries.length) return null;
  return <Section aria-label={title} className="gap-sm">
    <h3 className="text-body-sm font-semibold text-muted-foreground">{title}</h3>
    <List variant="plain" className="space-y-0 divide-y divide-border-subtle rounded-control border border-border-subtle bg-background">
      {entries.map(entry => <ListItem key={`${entry.writer_fleet_id}:${entry.key}`}
        className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-lg gap-y-xs px-lg py-md">
        <p className="col-span-full min-w-0 wrap-anywhere text-body-sm leading-body-sm text-foreground sm:col-span-1">{entry.content}</p>
        <div className="col-start-1 flex flex-wrap items-center gap-x-sm text-eyebrow leading-eyebrow text-muted-foreground">
          {forget ? <><span>{entry.visibility === VISIBILITY_WORKSPACE ? "Shared" : "Private"}</span><span aria-hidden="true">·</span></> : null}
          <span>Updated <Time value={entry.updated_at} format="date" /></span>
        </div>
        {forget ? <Button variant="ghost" size="compact" onClick={() => forget(entry)} aria-label={`${MEMORY_FORGET_LABEL} ${entry.key}`}
          className="col-start-2 row-start-2 px-sm hover:text-destructive sm:row-span-2 sm:row-start-1">{MEMORY_FORGET_LABEL}</Button> : null}
      </ListItem>)}
    </List>
  </Section>;
}
