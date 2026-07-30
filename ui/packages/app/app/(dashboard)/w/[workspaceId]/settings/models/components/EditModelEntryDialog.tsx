"use client";

import { useId, useState } from "react";
import {
  Alert,
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Input,
  Label,
  Spinner,
} from "@agentsfleet/design-system";
import { replaceSecretAction, updateModelEntryAction } from "../actions";
import { presentErrorString } from "@/lib/errors";
import { SECRET_KIND } from "@/lib/api/secrets";
import { OPENAI_COMPATIBLE_PROVIDER, SECRET_FIELD } from "@/lib/types";
import type { TenantModelEntry } from "@/lib/types";
import { isHttpsUrl, BASE_URL_NOT_HTTPS } from "../lib/custom-endpoint";
import { captureKeyRotated, captureModelChanged } from "../lib/track";
import ProviderModelSelect from "./ProviderModelSelect";

type Props = {
  workspaceId: string;
  target: TenantModelEntry | null;
  onOpenChange: (open: boolean) => void;
  onSaved: () => void;
  /** A write committed but the save did not complete — re-read the table so it
   *  shows what the server actually holds. Distinct from `onSaved`, which also
   *  means "close the dialog". Mirrors the refresh-even-on-failure idiom the
   *  table already applies to its own actions (`ModelsRegistryTable.onSwitchEntry`). */
  onCommitted: () => void;
};

const EDIT_MODEL_ACTION = "change the model";
const REPLACE_ACTION = "replace the credential";
const KEY_REQUIRED_TO_REPLACE =
  "Re-enter the API key. Replacing resends the whole credential, and a stored key can never be read back — leaving it blank would delete it.";
const CUSTOM_SECRET_HINT =
  "This entry uses an opaque secret the dashboard cannot recompose. Replace its value with: agentsfleet secret update";

// Rendered only while `target` is non-null (see the Dialog body below), so
// every field here takes `target` directly — no null branch to guard. Keyed
// by `target.id` at the call site, so React remounts fresh state whenever the
// target row changes without needing a re-seeding effect.
//
// The same form as Add, prefilled from the entry row the table already holds
// — provider, base URL, model — with the key blank because a stored secret is
// never readable. Replacement is total, so any secret-side change requires the
// key and sends ONE whole-body write.
//
// Save can issue two writes to two different daemon resources, which cannot
// share a transaction from here. So the order is chosen by what a stranded
// write costs: the ENTRY writes first because it is the recoverable one — a
// single row, visible in the table, changed back in two clicks — and the SECRET
// writes last because it is neither. A secret is shared by every entry
// referencing it and can never be read back to restore, so a stranded
// credential rotation is invisible, wide, and permanent. Ordering cannot make
// the pair atomic; it only decides which half can be left behind.
function EditForm({
  workspaceId,
  target,
  onOpenChange,
  onSaved,
  onCommitted,
}: {
  workspaceId: string;
  target: TenantModelEntry;
  onOpenChange: (open: boolean) => void;
  onSaved: () => void;
  onCommitted: () => void;
}) {
  const uid = useId();
  const [model, setModel] = useState(target.model_id);
  const [baseUrl, setBaseUrl] = useState(target.base_url ?? "");
  const [apiKey, setApiKey] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const isCustom = target.kind === SECRET_KIND.custom_endpoint;
  // An opaque secret has a shape only its author knows; the dashboard cannot
  // rebuild the body, so the secret side is read-only here and the CLI owns
  // replacement. Model edits (an entry-side write) remain available.
  const isOpaque = target.kind === SECRET_KIND.custom_secret;

  const modelChanged = model.trim() !== "" && model.trim() !== target.model_id;
  const keyEntered = apiKey.trim() !== "";
  const baseUrlChanged = isCustom && baseUrl.trim() !== (target.base_url ?? "");
  const secretTouched = !isOpaque && (keyEntered || baseUrlChanged);
  const canSubmit = model.trim() !== "" && (modelChanged || secretTouched);

  /** The whole replacement body, composed exactly as Add composes a create
   *  body — the two verbs must not disagree about what a secret is. */
  function composeReplacement(): Record<string, unknown> {
    if (isCustom) {
      const data: Record<string, unknown> = {
        [SECRET_FIELD.provider]: OPENAI_COMPATIBLE_PROVIDER,
        [SECRET_FIELD.baseUrl]: baseUrl.trim(),
      };
      if (keyEntered) data[SECRET_FIELD.apiKey] = apiKey.trim();
      return data;
    }
    return {
      [SECRET_FIELD.provider]: target.provider ?? "",
      [SECRET_FIELD.apiKey]: apiKey.trim(),
    };
  }

  // Only wired to the Save button below, disabled whenever `pending ||
  // !canSubmit` — no redundant re-check needed here.
  async function save() {
    setPending(true);
    setError(null);
    try {
      // EVERY validation runs before EITHER write. A check that fired between
      // them would strand the first write for a reason the caller could have
      // been given before anything was written.
      if (secretTouched) {
        if (isCustom && !isHttpsUrl(baseUrl.trim())) {
          setError(BASE_URL_NOT_HTTPS);
          return;
        }
        // Whole-body replace cannot preserve a key it can never read back, so a
        // secret that HAS a stored key must have it re-entered on any change —
        // otherwise changing only the base URL would silently drop the key and
        // 401 every model sharing it. (Reachable only for custom endpoints; a
        // named provider's `secretTouched` already implies a key was typed.)
        if (target.has_key && !keyEntered) {
          setError(KEY_REQUIRED_TO_REPLACE);
          return;
        }
      }

      // The recoverable write first (see the ordering note above the component).
      if (modelChanged) {
        const updated = await updateModelEntryAction(target.id, { model_id: model.trim() });
        if (!updated.ok) {
          setError(presentErrorString({ errorCode: updated.errorCode, message: updated.error, action: EDIT_MODEL_ACTION }));
          return;
        }
        captureModelChanged({ provider: target.provider ?? "", model: model.trim() });
      }

      // The permanent write last.
      if (secretTouched) {
        const replaced = await replaceSecretAction(workspaceId, target.secret_ref, composeReplacement());
        if (!replaced.ok) {
          setError(presentErrorString({ errorCode: replaced.errorCode, message: replaced.error, action: REPLACE_ACTION }));
          // The entry write above may already have committed. Re-read rather
          // than narrate it: the table then shows the model the server actually
          // holds, which is the same "mirror backend reality regardless of
          // outcome" rule its own actions follow. The dialog stays open on the
          // error so the credential can be retried.
          if (modelChanged) onCommitted();
          return;
        }
        if (keyEntered) captureKeyRotated(target.provider ?? "");
      }
      onSaved();
    } finally {
      setPending(false);
    }
  }

  return (
    <>
      <DialogHeader>
        <DialogTitle>{`Edit "${target.model_id}"`}</DialogTitle>
        <DialogDescription>
          Change the model, or replace the shared credential. Replacing resends the whole secret.
        </DialogDescription>
      </DialogHeader>
      <div className="space-y-3">
        <ProviderModelSelect id={`${uid}-model`} provider={target.provider} model={model} onModelChange={setModel} />
        {isCustom ? (
          <div className="space-y-2">
            <Label htmlFor={`${uid}-base-url`}>Base URL</Label>
            <Input
              id={`${uid}-base-url`}
              value={baseUrl}
              onChange={(e) => setBaseUrl(e.target.value)}
              spellCheck={false}
              autoComplete="off"
              className="font-mono"
            />
          </div>
        ) : null}
        {isOpaque ? (
          <p className="text-xs text-muted-foreground font-mono">{CUSTOM_SECRET_HINT}</p>
        ) : (
          <div className="space-y-2">
            <Label htmlFor={`${uid}-key`}>API key</Label>
            <Input
              id={`${uid}-key`}
              type="password"
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              placeholder={target.has_key ? "Re-enter the key (replacing resends the whole credential)" : isCustom ? "Blank for a keyless endpoint" : "Enter the key to replace the credential"}
              spellCheck={false}
              autoComplete="off"
              className="font-mono"
            />
            <p className="text-xs text-muted-foreground">
              Replacing updates every entry sharing this credential, not just this row.
            </p>
          </div>
        )}
      </div>
      {error ? <Alert variant="destructive" className="text-xs">{error}</Alert> : null}
      <DialogFooter>
        <Button type="button" variant="outline" disabled={pending} onClick={() => onOpenChange(false)}>
          Cancel
        </Button>
        <Button type="button" disabled={pending || !canSubmit} onClick={() => void save()}>
          {pending ? <Spinner size="sm" srLabel="Saving" /> : null}
          Save
        </Button>
      </DialogFooter>
    </>
  );
}

export default function EditModelEntryDialog({ workspaceId, target, onOpenChange, onSaved, onCommitted }: Props) {
  return (
    <Dialog open={target !== null} onOpenChange={onOpenChange}>
      <DialogContent>
        {target ? (
          <EditForm
            key={target.id}
            workspaceId={workspaceId}
            target={target}
            onOpenChange={onOpenChange}
            onSaved={onSaved}
            onCommitted={onCommitted}
          />
        ) : null}
      </DialogContent>
    </Dialog>
  );
}
