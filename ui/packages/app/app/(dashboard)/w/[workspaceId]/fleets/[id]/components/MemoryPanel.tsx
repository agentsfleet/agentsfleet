"use client";

import { useState, useTransition } from "react";
import {
  Alert,
  Badge,
  Button,
  Card,
  ConfirmDialog,
  EmptyState,
  List,
  ListItem,
  Time,
} from "@agentsfleet/design-system";
import { BrainIcon } from "lucide-react";
import type { MemoryEntry } from "@/lib/types";
import { forgetMemoryAction } from "../../actions";
import { captureProductEvent } from "@/lib/analytics/posthog";
import { EVENTS } from "@/lib/analytics/events";
import { presentErrorString } from "@/lib/errors";
import {
  MEMORY_EMPTY_DESCRIPTION,
  MEMORY_EMPTY_TITLE,
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

type Props = {
  workspaceId: string;
  fleetId: string;
  entries: MemoryEntry[];
};

export default function MemoryPanel({ workspaceId, fleetId, entries: initial }: Props) {
  const [entries, setEntries] = useState<MemoryEntry[]>(initial);
  const [pendingKey, setPendingKey] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [, startTransition] = useTransition();

  function forget(key: string) {
    setNotice(null);
    startTransition(async () => {
      const result = await forgetMemoryAction(workspaceId, fleetId, key);
      setPendingKey(null);
      if (result.ok) {
        setEntries((prev) => prev.filter((e) => e.key !== key));
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
    });
  }

  return (
    <Card className="flex flex-col gap-md bg-card p-4" aria-label={MEMORY_PANEL_TITLE}>
      <span className="font-mono text-sm font-medium text-foreground">{MEMORY_PANEL_TITLE}</span>
      {notice ? <Alert variant="warning">{notice}</Alert> : null}
      {entries.length === 0 ? (
        <EmptyState icon={<BrainIcon size={28} />} title={MEMORY_EMPTY_TITLE} description={MEMORY_EMPTY_DESCRIPTION} />
      ) : (
        <List variant="ordered" className="flex list-none flex-col gap-2 space-y-0 pl-0">
          {entries.map((entry) => (
            <ListItem key={entry.key}>
              <MemoryRow entry={entry} onForget={() => setPendingKey(entry.key)} />
            </ListItem>
          ))}
        </List>
      )}
      <ConfirmDialog
        open={pendingKey !== null}
        onOpenChange={() => setPendingKey(null)}
        intent="destructive"
        title={MEMORY_FORGET_DIALOG_TITLE}
        description={MEMORY_FORGET_DIALOG_DESCRIPTION}
        confirmLabel={MEMORY_FORGET_CONFIRM_LABEL}
        onConfirm={pendingKey ? () => forget(pendingKey) : undefined}
      />
    </Card>
  );
}

function MemoryRow({ entry, onForget }: { entry: MemoryEntry; onForget: () => void }) {
  return (
    <Card className="flex items-start justify-between gap-md p-3">
      <div className="flex min-w-0 flex-col gap-xs">
        <p className="break-words text-sm text-foreground">{entry.content}</p>
        <div className="flex flex-wrap items-center gap-md">
          <Badge variant="default">{entry.category}</Badge>
          <Time
            value={new Date(entry.updated_at)}
            format="relative"
            tooltip={false}
            className="font-mono text-xs text-muted-foreground tabular-nums"
          />
        </div>
      </div>
      <Button type="button" variant="ghost" size="sm" onClick={onForget} aria-label={`${MEMORY_FORGET_LABEL} ${entry.key}`}>
        {MEMORY_FORGET_LABEL}
      </Button>
    </Card>
  );
}
