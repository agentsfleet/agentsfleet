"use client";

import type { FormEvent, ReactNode } from "react";
import { useEffect, useId, useRef, useState } from "react";
import { Alert, Button, Checkbox, Label, Spinner, Textarea } from "@agentsfleet/design-system";
import { useCurrentUser } from "@/lib/auth/client";
import { setupErrorMessage } from "@/lib/api/draft-request";
import { useStoredSecrets } from "@/app/(dashboard)/w/[workspaceId]/settings/models/components/use-stored-secrets";
import { SECRETS_LOAD } from "@/app/(dashboard)/w/[workspaceId]/settings/models/components/secrets-load";

type Props = {workspace: string; kind: "repositories" | "credentials"; selected: readonly string[];
  disabled: boolean; pending: boolean; onRetry: () => Promise<void>; onReload: () => Promise<void>;
  onApply: (names: readonly string[]) => Promise<void>; onCancel: () => void};
const signature = (names: readonly string[]) => JSON.stringify([...names].sort());

export function SetupSelectionEditor(props: Props) {
  const {userId} = useCurrentUser();
  return props.kind === "repositories" ? <RepositorySelection {...props} />
    : <SecretSelection key={`${props.workspace}:${userId}`} {...props} subject={userId} />;
}

function RepositorySelection({selected, onApply, ...actions}: Props) {
  const inputId = useId();
  const [names, setNames] = useState(selected.join("\n"));
  const [baseline] = useState(() => signature(selected));
  const conflict = baseline !== signature(selected) && !actions.disabled;
  return <SelectionForm {...actions} disabled={actions.disabled || conflict} onApply={() => onApply([...new Set(names.split(/\r?\n/).map(name => name.trim()).filter(Boolean))])}>
    {conflict ? <Alert variant="warning">The repositories changed. Your edits are still here. Close this editor to review the latest setup.</Alert> : null}
    <Label htmlFor={inputId}>Repositories</Label>
    <Textarea id={inputId} value={names} onChange={event => setNames(event.target.value)} rows={5} maxLength={4096} placeholder="owner/repository" />
    <p className="text-label text-muted-foreground">One repository per line. The setup review checks its access.</p>
    <p className="text-eyebrow text-muted-foreground">New bindings use read access. Existing write access stays unchanged.</p>
  </SelectionForm>;
}

function SecretSelection({workspace, subject, selected, onApply, ...actions}: Props & {subject: string | null}) {
  const {secrets, secretsLoad, refreshSecrets} = useStoredSecrets(workspace, subject);
  const [load] = useState(() => refreshSecrets);
  const [names, setNames] = useState(() => new Set(selected));
  const [baseline] = useState(() => signature(selected));
  useEffect(() => {load();}, [load]);
  const ready = secretsLoad === SECRETS_LOAD.ready;
  const options = [...new Set([...selected, ...secrets.map(secret => secret.name)])];
  const conflict = baseline !== signature(selected) && !actions.disabled;
  return <SelectionForm {...actions} onApply={() => onApply([...names])} disabled={!ready || actions.disabled || conflict}>
    {conflict ? <Alert variant="warning">The selected secrets changed. Your edits are still here. Close this editor to review the latest setup.</Alert> : null}
    <p className="text-label text-muted-foreground">Choose existing secrets. Their values stay hidden.</p>
    {secretsLoad === SECRETS_LOAD.error ? <Alert variant="destructive">Secrets could not be loaded.<Button variant="link" onClick={refreshSecrets}>Retry secrets</Button></Alert>
      : !ready ? <Spinner label="Loading secret names" /> : options.length ? <ul className="space-y-md">{options.map(name => <li key={name}>
        <label className="flex min-h-11 items-center gap-md"><Checkbox checked={names.has(name)} onCheckedChange={checked => setNames(previous => {
          const next = new Set(previous);if (checked === true) next.add(name);else next.delete(name);return next;
        })} /><span className="break-words text-label">{name}</span></label>
      </li>)}</ul> : <p className="text-label text-muted-foreground">No secrets are available in this workspace.</p>}
  </SelectionForm>;
}

function SelectionForm({children, disabled, pending, onApply, onRetry, onReload, onCancel}: {children: ReactNode; disabled: boolean; pending: boolean;
  onApply: () => Promise<void>; onRetry: () => Promise<void>; onReload: () => Promise<void>; onCancel: () => void}) {
  const inFlight = useRef(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const apply = async (event: FormEvent): Promise<void> => {
    event.preventDefault();if (inFlight.current || disabled && !pending) return;
    inFlight.current = true;setBusy(true);setError(null);
    try {await (pending ? onRetry() : onApply());} catch (cause) {setError(setupErrorMessage(cause));}
    finally {inFlight.current = false;setBusy(false);}
  };
  return <form className="space-y-md" onSubmit={event => {void apply(event);}}>
    <fieldset disabled={busy || pending} className="space-y-md">{children}</fieldset>
    {error ? <Alert variant="destructive">{error}{!pending ? <Button type="button" variant="link" disabled={busy} onClick={() => {void onReload().catch(cause => setError(setupErrorMessage(cause)));}}>Refresh setup</Button> : null}</Alert> : null}
    <div className="flex justify-end gap-md"><Button type="button" variant="outline" disabled={busy} onClick={onCancel}>Cancel</Button>
      <Button type="submit" disabled={busy || disabled && !pending}>{busy ? "Applying…" : pending ? "Retry update" : "Apply selection"}</Button></div>
  </form>;
}
