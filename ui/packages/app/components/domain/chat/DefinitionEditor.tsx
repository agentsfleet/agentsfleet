"use client";

import type { DraftClient } from "@/lib/api/drafts";
import type { DraftView } from "@/lib/api/draft-state";
import { useEffect, useId, useState } from "react";
import { Alert, Button, Input, Label, Textarea } from "@agentsfleet/design-system";
import { DRAFT_STATE } from "@/lib/api/draft-types";
import { savedDraft } from "@/lib/api/draft-continuation";
import { setupErrorMessage } from "@/lib/api/draft-request";
import { chatRecovery } from "@/lib/api/draft-recovery";
import { initialDefinition } from "./chat-defaults";

type Props = {client: DraftClient; view: DraftView; onUpdated: () => void; onDirtyChange?: (dirty: boolean) => void; onCancel?: () => void};

export function DefinitionEditor({client, view, onUpdated, onDirtyChange, onCancel}: Props) {
  const inputId = useId();
  const initial = view.draft?.candidate?.definition ?? view.preview?.definition ?? view.fleet?.definition ?? initialDefinition();
  const form = useEditorFields({instructions: initial.instructions_md,
    name: view.draft?.review?.effective.instance_name ?? view.preview?.instanceName ?? view.fleet?.name ?? String(initial.config.name)});
  const {instructions, name} = form.values;
  const [error, setError] = useState<string | null>(null);
  const [savePending, setSavePending] = useState(false);
  const hasWork = form.dirty || savePending;
  const retryApply = view.pending && savePending;
  useEffect(() => {onDirtyChange?.(hasWork);}, [hasWork, onDirtyChange]);
  const recovery = chatRecovery(view);
  const locked = form.conflict || view.busy || (view.pending && !retryApply) || recovery.pending || recovery.blocked
    || Boolean(view.draft && (!view.draft.candidate || view.draft.state !== DRAFT_STATE.editing && !savedDraft(view)));
  const apply = async (): Promise<void> => {
    setError(null);
    try {
      if (retryApply) await client.retry();
      else {
        const updating = Boolean(view.preview?.setupRevision || view.draft?.linked_fleet_id || view.fleet);
        if (form.dirty) {
          await client.edit({definition: {...initial, instructions_md: instructions}, instanceName: name});
          setSavePending(true);
        }
        await client.refreshSetup();
        if (updating) await client.apply();
      }
      setSavePending(false);
      onUpdated();
    } catch (cause) {setError(setupErrorMessage(cause));}
  };
  return <div className="flex min-h-0 flex-1 flex-col gap-md">
    {form.conflict ? <Alert variant="warning">Setup chat changed these instructions. Your edits are still here.
      <Button variant="outline" size="sm" onClick={() => {form.reset(); setError(null);}}>Discard manual changes and load latest</Button>
    </Alert> : null}
    <div className="space-y-xs"><Label htmlFor={`${inputId}-name`}>Fleet name</Label><Input id={`${inputId}-name`} disabled={view.busy || view.pending} value={name} onChange={event => form.change("name", event.target.value)} maxLength={64} /></div>
    <div className="flex min-h-0 flex-1 flex-col gap-xs"><Label htmlFor={`${inputId}-instructions`}>Instructions</Label>
      <Textarea id={`${inputId}-instructions`} disabled={view.busy || view.pending} rows={18} className="min-h-64 flex-1 resize-y leading-prose" value={instructions} onChange={event => form.change("instructions", event.target.value)} /></div>
    {error ? <Alert variant="destructive">{error}</Alert> : null}
    <div className="flex flex-wrap items-center gap-sm">
      <div className="ml-auto flex gap-sm">{onCancel ? <Button variant="ghost" disabled={view.busy} onClick={onCancel}>Cancel</Button> : null}
        <Button disabled={locked || !hasWork} onClick={() => {void apply();}}>{retryApply ? "Retry save" : "Save changes"}</Button></div>
    </div>
  </div>;
}

type EditorFields = {instructions: string; name: string};

function useEditorFields(current: EditorFields) {
  const signature = JSON.stringify(current);
  const [form, setForm] = useState({baseline: signature, values: current});
  const valuesSignature = JSON.stringify(form.values);
  const changed = form.baseline !== signature;
  const edited = valuesSignature !== form.baseline;
  const rebase = changed && (!edited || valuesSignature === signature);
  if (rebase) setForm({baseline: signature, values: current});
  const values = rebase ? current : form.values;
  return {values, dirty: edited && !rebase, conflict: changed && edited && !rebase,
    change: (key: keyof EditorFields, value: string) => setForm({...form, values: {...values, [key]: value}}),
    reset: () => setForm({baseline: signature, values: current})};
}
