"use client";

import type { DraftClient } from "@/lib/api/drafts";
import type { DraftView } from "@/lib/api/draft-state";
import { useEffect, useId, useState } from "react";
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger, Alert, Button, Input, Label, Textarea } from "@agentsfleet/design-system";
import { DefinitionSchema, DRAFT_STATE } from "@/lib/api/draft-types";
import { draftTriggerError } from "@/lib/api/draft-triggers";
import { setupErrorMessage } from "@/lib/api/draft-request";
import { chatRecovery } from "@/lib/api/draft-recovery";
import { initialDefinition } from "./chat-defaults";

export function DefinitionEditor({client, view, onUpdated, onDirtyChange}: {client: DraftClient; view: DraftView; onUpdated: () => void; onDirtyChange?: (dirty: boolean) => void}) {
  const inputId = useId();
  const initial = view.draft?.candidate?.definition ?? view.preview?.definition ?? initialDefinition();
  const form = useEditorFields({instructions: initial.instructions_md, config: JSON.stringify(initial.config, null, 2),
    name: view.draft?.review?.effective.instance_name ?? view.preview?.instanceName ?? String(initial.config.name)});
  const {instructions, config, name} = form.values;
  const [error, setError] = useState<string | null>(null);
  const dirty = form.dirty;
  useEffect(() => {onDirtyChange?.(dirty);}, [dirty, onDirtyChange]);
  const recovery = chatRecovery(view);
  const apply = async (): Promise<void> => {
    setError(null);
    try {
      const parsedConfig = DefinitionSchema.shape.config.safeParse(JSON.parse(config));
      if (!parsedConfig.success) { setError("Runtime config must be a JSON object."); return; }
      const triggerError = draftTriggerError(parsedConfig.data);
      if (triggerError) { setError(triggerError); return; }
      const definition = {...initial, instructions_md: instructions, config: parsedConfig.data};
      await client.edit({definition, instanceName: name});
      await client.refreshSetup();
      onUpdated();
    } catch (cause) { setError(cause instanceof SyntaxError ? "Config must be valid JSON." : setupErrorMessage(cause)); }
  };
  return <div className="space-y-md">
    {!view.draft && !view.preview ? <p className="text-sm text-muted-foreground">Defaults: chat and steer messages only, no tools, daily ceiling of 1,000,000,000 nanos ($1), and no monthly ceiling. Empty instructions remain editable and cannot be saved.</p> : null}
    {form.conflict ? <Alert variant="warning">Setup chat changed this setup while you were editing. Your manual edits are still here.
      <Button variant="outline" size="sm" onClick={() => {form.reset(); setError(null);}}>Discard manual changes and load latest</Button>
    </Alert> : null}
    <div className="space-y-xs"><Label htmlFor={`${inputId}-instance-name`}>Fleet name</Label><Input id={`${inputId}-instance-name`} value={name} onChange={(event) => form.change("name", event.target.value)} maxLength={64} /></div>
    <div className="space-y-xs"><Label htmlFor={`${inputId}-instructions`}>Instructions</Label><Textarea id={`${inputId}-instructions`} rows={8} value={instructions} onChange={(event) => form.change("instructions", event.target.value)} /></div>
    <Accordion type="single" collapsible><AccordionItem value="advanced"><AccordionTrigger>Advanced settings</AccordionTrigger>
      <AccordionContent><div className="space-y-xs"><Label htmlFor={`${inputId}-config`}>Runtime config</Label><Textarea id={`${inputId}-config`} rows={10} className="font-mono" value={config} onChange={(event) => form.change("config", event.target.value)} /></div></AccordionContent>
    </AccordionItem></Accordion>
    {error ? <Alert variant="destructive">{error}</Alert> : null}
    <Button disabled={form.conflict || view.busy || view.pending || recovery.pending || recovery.blocked || Boolean(view.draft && (!view.draft.candidate || view.draft.state !== DRAFT_STATE.editing))} onClick={() => { void apply(); }}>Update instructions</Button>
  </div>;
}

type EditorFields = {instructions: string; config: string; name: string};

function useEditorFields(current: EditorFields) {
  const signature = JSON.stringify(current);
  const [form, setForm] = useState({baseline: signature, values: current});
  const valuesSignature = JSON.stringify(form.values);
  const changed = form.baseline !== signature;
  const edited = valuesSignature !== form.baseline;
  const rebase = changed && (!edited || valuesSignature === signature);
  // Compare against the form's own baseline so chat cannot turn untouched values into edits.
  if (rebase) setForm({baseline: signature, values: current});
  const values = rebase ? current : form.values;
  const dirty = edited && !rebase;
  return {values, dirty, conflict: changed && dirty,
    change: (key: keyof EditorFields, value: string) => setForm({...form, values: {...values, [key]: value}}),
    reset: () => setForm({baseline: signature, values: current})};
}
