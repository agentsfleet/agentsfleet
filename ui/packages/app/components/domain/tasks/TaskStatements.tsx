"use client";

import type { DurableTaskDetail, DurableTaskStatement } from "@/lib/api/tasks";
import { useEffect, useId, useRef, useState } from "react";
import { Button, Label, List, ListItem, Textarea } from "@agentsfleet/design-system";
import { editDurableTaskStatements } from "@/lib/api/tasks";
import { TaskStatement } from "./TaskStatement";

type Props = {workspaceId: string; fleetId: string; detail: DurableTaskDetail; enabled: boolean; readOnly: boolean; onChanged: () => void};
type Status = {kind: "idle" | "saving" | "stale"} | {kind: "failed"; message: string};

const TERMINAL = ["done", "cancelled", "exhausted"];
const STALE = "This task changed while you edited it, so it was read again. Your wording is kept: save again to apply it.";

/** A task's completion statements; a person rewords them with one `PATCH F/durable_tasks/{id}` against the generation they read. */
export function DurableTaskStatements({workspaceId, fleetId, detail, enabled, readOnly, onChanged}: Props) {
  const id = useId();
  const [editing, setEditing] = useState<readonly DurableTaskStatement[] | null>(null);
  const [status, setStatus] = useState<Status>({kind: "idle"});
  const active = useRef<AbortController | null>(null);
  useEffect(() => () => active.current?.abort(), []);
  const editable = !readOnly && !TERMINAL.includes(detail.context.state);
  const save = async (questions: readonly DurableTaskStatement[]): Promise<void> => {
    if (active.current) return;
    const pending = new AbortController();
    active.current = pending;
    setStatus({kind: "saving"});
    const result = await editDurableTaskStatements(workspaceId, fleetId, detail, questions, pending.signal);
    active.current = null;
    if (pending.signal.aborted) return;
    if (result.kind === "confirmed") {setEditing(null); setStatus({kind: "idle"}); onChanged(); return;}
    if (result.reason !== "changed") {setStatus({kind: "failed", message: failure(result.reason)}); return;}
    // Another edit won: read the task again, and leave this person's words where they are.
    setStatus({kind: "stale"});
    onChanged();
  };
  return <div className="space-y-sm">
    <h3 className="font-medium">What counts as done?</h3>
    {editing ? <form className="space-y-md" aria-label="Edit completion statements" onSubmit={event => {event.preventDefault(); void save(editing);}}>
      {editing.map((statement, index) => <div key={statement.name} className="space-y-xs">
        <Label htmlFor={`${id}-${index}`}>Statement {index + 1}</Label>
        <Textarea id={`${id}-${index}`} rows={2} value={statement.question.instructions} disabled={status.kind === "saving"}
          onChange={event => {
            const instructions = event.currentTarget.value;
            setEditing(editing.map((row, at) => at === index ? {...row, question: {...row.question, instructions}} : row));
          }} />
      </div>)}
      <div className="flex gap-xs">
        <Button type="submit" variant="ghost" size="sm"
          disabled={!enabled || status.kind === "saving" || editing.some(row => row.question.instructions.trim() === "")}>
          {status.kind === "saving" ? "Saving statements…" : "Save statements"}</Button>
        <Button type="button" variant="ghost" size="sm" disabled={status.kind === "saving"}
          onClick={() => {setEditing(null); setStatus({kind: "idle"});}}>Cancel</Button>
      </div>
    </form> : <>
      <List variant="plain" className="space-y-md">{detail.context.questions.map((statement, index) => <ListItem key={statement.name}>
        <TaskStatement number={index + 1} statement={statement} />
      </ListItem>)}</List>
      {editable ? <Button variant="ghost" size="sm" disabled={!enabled} onClick={() => setEditing(detail.context.questions)}>Edit statements</Button> : null}
    </>}
    {status.kind === "stale" ? <output className="block text-eyebrow text-muted-foreground">{STALE}</output> : null}
    {status.kind === "failed" ? <output className="block text-eyebrow text-muted-foreground">{status.message}</output> : null}
  </div>;
}

function failure(reason: "access" | "limited" | "unconfirmed" | "invalid"): string {
  switch (reason) {
    case "access": return "Only a signed-in person can change these statements. Sign in or ask a workspace owner.";
    case "limited": return "The task reached a limit. Its statements are unchanged.";
    case "invalid":
    case "unconfirmed": return "The change could not be confirmed. Refresh progress before trying again.";
  }
}
