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
};

const EDIT_MODEL_ACTION = "change the model";
const REPLACE_ACTION = "replace the credential";
const CUSTOM_SECRET_HINT =
  "This entry uses an opaque secret the dashboard cannot recompose. Replace its value with: agentsfleet secret update";

// Rendered only while `target` is non-null (see the Dialog body below), so
// every field here takes `target` directly — no null branch to guard. Keyed
// by `target.id` at the call site, so React remounts fresh state whenever the
// target row changes without needing a re-seeding effect.
//
// The same form as Add, prefilled from the entry row the table already holds
// — provider, base URL, model — with the key blank because a stored secret is
// never readable. Replacement is total, so any secret-side change requires
// the key and sends ONE whole-body write. The secret writes FIRST: a later
// entry-write failure leaves the table consistent (the rename never
// committed), so no partial-success path exists.
function EditForm({
  workspaceId,
  target,
  onOpenChange,
  onSaved,
}: {
  workspaceId: string;
  target: TenantModelEntry;
  onOpenChange: (open: boolean) => void;
  onSaved: () => void;
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
      if (secretTouched) {
        if (isCustom && !isHttpsUrl(baseUrl.trim())) {
          setError(BASE_URL_NOT_HTTPS);
          return;
        }
        const replaced = await replaceSecretAction(workspaceId, target.secret_ref, composeReplacement());
        if (!replaced.ok) {
          setError(presentErrorString({ errorCode: replaced.errorCode, message: replaced.error, action: REPLACE_ACTION }));
          return;
        }
        if (keyEntered) captureKeyRotated(target.provider ?? "");
      }
      if (modelChanged) {
        const updated = await updateModelEntryAction(target.id, { model_id: model.trim() });
        if (!updated.ok) {
          setError(presentErrorString({ errorCode: updated.errorCode, message: updated.error, action: EDIT_MODEL_ACTION }));
          return;
        }
        captureModelChanged({ provider: target.provider ?? "", model: model.trim() });
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
              placeholder={isCustom ? "Blank for a keyless endpoint" : "Enter the key to replace the credential"}
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

export default function EditModelEntryDialog({ workspaceId, target, onOpenChange, onSaved }: Props) {
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
          />
        ) : null}
      </DialogContent>
    </Dialog>
  );
}
