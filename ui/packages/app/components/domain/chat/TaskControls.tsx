"use client";

import { useEffect, useRef, useState } from "react";
import { Button } from "@agentsfleet/design-system";
import { controlTask, type TaskChange, type TaskControl, type TaskDetail } from "@/lib/api/tasks";

type Props = {workspaceId: string; fleetId: string; detail: TaskDetail; enabled: boolean; onChanged: () => void};
type Action = {state: TaskControl; label: string};
type Status = {kind: "idle"} | {kind: "sending"; label: string} | {kind: "failed"; message: string};

export function TaskControls({workspaceId, fleetId, detail, enabled, onChanged}: Props) {
  const [status, setStatus] = useState<Status>({kind: "idle"});
  const active = useRef<AbortController | null>(null);
  useEffect(() => () => active.current?.abort(), []);
  useEffect(() => {setStatus(previous => previous.kind === "sending" ? previous : {kind: "idle"});}, [detail]);
  const terminal = ["done", "cancelled", "exhausted"].includes(detail.context.state);
  if (terminal) return null;
  const actions: Action[] = detail.context.state === "paused" || detail.context.state === "blocked"
    ? [{state: "ready", label: "Resume"}, {state: "cancelled", label: "Cancel work"}]
    : [{state: "paused", label: "Pause"}, {state: "cancelled", label: "Cancel work"}];
  const change = async (action: Action): Promise<void> => {
    if (active.current || !enabled || !detail.etag || status.kind === "failed") return;
    const pending = new AbortController();
    active.current = pending;
    setStatus({kind: "sending", label: action.label});
    const result = await controlTask(workspaceId, fleetId, detail, action.state, pending.signal);
    active.current = null;
    if (pending.signal.aborted) return;
    if (result.kind === "confirmed") {
      setStatus({kind: "idle"});
      onChanged();
    } else {setStatus({kind: "failed", message: failure(result)});}
  };
  return <div className="space-y-xs" aria-label="Work controls">
    <div className="flex flex-wrap items-center gap-xs">
      {actions.map(action => <Button key={action.state} variant="ghost" size="sm"
        disabled={!enabled || !detail.etag || status.kind !== "idle"} onClick={() => {void change(action);}}>{action.label}</Button>)}
      {status.kind === "sending" ? <output data-chat-loading="true" className="inline-flex items-center gap-xs text-eyebrow text-muted-foreground">{status.label}…</output> : null}
      {status.kind === "failed" || !detail.etag ? <Button variant="ghost" size="sm" onClick={onChanged}>Refresh progress</Button> : null}
    </div>
    {status.kind === "failed" ? <output className="block text-eyebrow text-muted-foreground">{status.message}</output> : null}
  </div>;
}

function failure(result: Extract<TaskChange, {kind: "failed"}>): string {
  switch (result.reason) {
    case "access": return "You don’t have access to change this work. Sign in or ask a workspace owner.";
    case "changed": return "This work changed. Refresh its progress before choosing an action.";
    case "limited": return "The work reached a limit. Its progress has been retained.";
    case "invalid":
    case "unconfirmed": return "The change could not be confirmed. Refresh progress before trying again.";
  }
}
