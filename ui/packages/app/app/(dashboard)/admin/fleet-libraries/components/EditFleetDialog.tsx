"use client";

import type { PlatformEntry, PlatformPatch } from "@/lib/api/platform-library-types";
import { useId, useState } from "react";
import { useRouter } from "next/navigation";
import { z } from "zod";
import { ActionForm, Alert, Button, Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle, Input, Label, Spinner, Textarea } from "@agentsfleet/design-system";
import { presentErrorString, unexpectedErrorMessage } from "@/lib/errors";
import { patchPlatformLibraryAction } from "../actions";
import { EDIT_DESCRIPTION, EDIT_DESCRIPTION_LABEL, EDIT_NAME_LABEL, EDIT_REASON_LABEL, EDIT_REASON_MISSING, EDIT_TITLE, PATCH_ACTION } from "../library-copy";

type Props = {entry: PlatformEntry; open: boolean; onOpenChange: (open: boolean) => void; onSaved: (entry: PlatformEntry) => void};
const CredentialsSchema = z.object({credentials: z.array(z.string())});

export default function EditFleetDialog(props: Props) {
  const state = useEditor(props);
  return <Dialog open={props.open} onOpenChange={(open) => { if (!state.pending) props.onOpenChange(open); }}>
    <DialogContent className="max-h-svh overflow-y-auto">
      <DialogHeader><DialogTitle>{EDIT_TITLE}</DialogTitle><DialogDescription>{EDIT_DESCRIPTION}</DialogDescription></DialogHeader>
      <ActionForm onSubmit={(event) => { event.preventDefault(); void state.save(); }}>
        <EditorFields state={state} />
        {state.error ? <Alert variant="destructive">{state.error}</Alert> : null}
        <DialogFooter>
          <Button type="button" variant="ghost" disabled={state.pending} onClick={() => props.onOpenChange(false)}>Cancel</Button>
          <Button type="submit" disabled={state.pending || !state.name.trim()}>{state.pending ? <Spinner size="sm" srLabel="Saving" /> : null}Save</Button>
        </DialogFooter>
      </ActionForm>
    </DialogContent>
  </Dialog>;
}

function useEditor({entry, onSaved, onOpenChange}: Props) {
  const router = useRouter();
  // Revalidation cannot change what this dialog displayed or its conditional tag.
  const [baseline] = useState(entry);
  const [name, setName] = useState(baseline.name);
  const [description, setDescription] = useState(baseline.description);
  const [reasons, setReasons] = useState(baseline.required_credentials_reasons);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  async function save() {
    setPending(true); setError(null);
    try {
      const patch: PlatformPatch = {};
      if (name !== baseline.name) patch.name = name;
      if (description !== baseline.description) patch.description = description;
      if (JSON.stringify(reasons) !== JSON.stringify(baseline.required_credentials_reasons)) patch.required_credentials_reasons = reasons;
      if (!Object.keys(patch).length) { onOpenChange(false); return; }
      const result = await patchPlatformLibraryAction(baseline.id, patch, baseline.etag);
      if (!result.ok) {
        setError(presentErrorString({errorCode: result.errorCode, message: result.error, action: PATCH_ACTION}));
        router.refresh(); return;
      }
      onSaved(result.data); onOpenChange(false);
    } catch (cause) {
      setError(unexpectedErrorMessage(cause, PATCH_ACTION));
      router.refresh();
    } finally { setPending(false); }
  }
  const parsed = CredentialsSchema.safeParse(baseline.requirements);
  return {name, setName, description, setDescription, reasons, setReasons, pending, error, save, credentials: parsed.success ? parsed.data.credentials : []};
}

function EditorFields({state}: {state: ReturnType<typeof useEditor>}) {
  const id = useId();
  return <div className="space-y-md">
    <div className="space-y-xs"><Label htmlFor={`${id}-name`}>{EDIT_NAME_LABEL}</Label><Input id={`${id}-name`} value={state.name} maxLength={100} disabled={state.pending} onChange={(event) => state.setName(event.target.value)} /></div>
    <div className="space-y-xs"><Label htmlFor={`${id}-description`}>{EDIT_DESCRIPTION_LABEL}</Label><Textarea id={`${id}-description`} value={state.description} maxLength={2000} disabled={state.pending} onChange={(event) => state.setDescription(event.target.value)} /></div>
    {state.credentials.length ? <div className="space-y-sm"><p className="text-sm font-medium">{EDIT_REASON_LABEL}</p>
      {state.credentials.map((credential) => <CredentialReason key={credential} credential={credential} value={state.reasons[credential] ?? ""} disabled={state.pending}
        onChange={(value) => state.setReasons((previous) => ({...previous, [credential]: value}))} />)}</div> : null}
  </div>;
}

function CredentialReason({credential, value, disabled, onChange}: {credential: string; value: string; disabled: boolean; onChange: (value: string) => void}) {
  const id = useId();
  return <div className="space-y-xs">
    <Label htmlFor={id}>{credential}</Label><Input id={id} value={value} maxLength={500} disabled={disabled} onChange={(event) => onChange(event.target.value)} />
    {!value.trim() ? <p className="text-xs text-muted-foreground">{EDIT_REASON_MISSING}</p> : null}
  </div>;
}
