"use client";

import type { DraftClient } from "@/lib/api/drafts";
import type { DraftView } from "@/lib/api/draft-state";
import { useEffect, useId, useState } from "react";
import { Alert, Button, Input, Label, Textarea } from "@agentsfleet/design-system";
import { DefinitionSchema, DRAFT_STATE } from "@/lib/api/draft-types";
import { draftTriggerError } from "@/lib/api/draft-triggers";
import { setupErrorMessage } from "@/lib/api/draft-request";
import { chatRecovery } from "@/lib/api/draft-recovery";
import { initialDefinition } from "./chat-defaults";

export function DefinitionEditor({client, view, onUpdated, onDirtyChange}: {client: DraftClient; view: DraftView; onUpdated: () => void; onDirtyChange?: (dirty: boolean) => void}) {
  const inputId = useId();
  const initial = view.draft?.candidate?.definition ?? view.preview?.definition ?? initialDefinition();
  const [instructions, setInstructions] = useState(initial.instructions_md);
  const [config, setConfig] = useState(JSON.stringify(initial.config, null, 2));
  const [name, setName] = useState(view.draft?.review?.effective.instance_name ?? view.preview?.instanceName ?? String(initial.config.name));
  const [error, setError] = useState<string | null>(null);
  const dirty = instructions !== initial.instructions_md || config !== JSON.stringify(initial.config, null, 2)
    || name !== (view.draft?.review?.effective.instance_name ?? view.preview?.instanceName ?? String(initial.config.name));
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
    <div className="space-y-xs"><Label htmlFor={`${inputId}-instance-name`}>Instance name</Label><Input id={`${inputId}-instance-name`} value={name} onChange={(event) => setName(event.target.value)} maxLength={64} /></div>
    <div className="space-y-xs"><Label htmlFor={`${inputId}-instructions`}>Instructions</Label><Textarea id={`${inputId}-instructions`} rows={8} value={instructions} onChange={(event) => setInstructions(event.target.value)} /></div>
    <div className="space-y-xs"><Label htmlFor={`${inputId}-config`}>Runtime config</Label><Textarea id={`${inputId}-config`} rows={10} className="font-mono" value={config} onChange={(event) => setConfig(event.target.value)} /></div>
    {error ? <Alert variant="destructive">{error}</Alert> : null}
    <Button disabled={view.busy || view.pending || recovery.pending || recovery.blocked || Boolean(view.draft && (!view.draft.candidate || view.draft.state !== DRAFT_STATE.editing))} onClick={() => { void apply(); }}>Update instructions</Button>
  </div>;
}
