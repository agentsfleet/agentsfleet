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
import { rotateSecretAction, updateModelEntryAction } from "../actions";
import { presentErrorString } from "@/lib/errors";
import type { TenantModelEntry } from "@/lib/types";
import { captureKeyRotated, captureModelChanged } from "../lib/track";
import ProviderModelSelect from "./ProviderModelSelect";

type Props = {
  workspaceId: string;
  target: TenantModelEntry | null;
  onOpenChange: (open: boolean) => void;
  onSaved: () => void;
};

const EDIT_MODEL_ACTION = "change the model";
const ROTATE_ACTION = "rotate the key";

// Rendered only while `target` is non-null (see the Dialog body below), so
// every field here takes `target` directly — no null branch to guard. Keyed
// by `target.id` at the call site, so React remounts fresh state (not a
// stale model/key from the previously edited row) whenever the target row
// changes without needing a re-seeding effect.
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
  const [apiKey, setApiKey] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const modelChanged = model.trim() !== "" && model.trim() !== target.model_id;
  const keyChanged = apiKey.trim() !== "";
  const canSubmit = model.trim() !== "" && (modelChanged || keyChanged);

  // Only wired to the Save button below, disabled whenever `pending ||
  // !canSubmit` — no redundant re-check needed here.
  async function save() {
    setPending(true);
    setError(null);
    try {
      if (modelChanged) {
        const updated = await updateModelEntryAction(target.id, { model_id: model.trim() });
        if (!updated.ok) {
          setError(presentErrorString({ errorCode: updated.errorCode, message: updated.error, action: EDIT_MODEL_ACTION }));
          return;
        }
        captureModelChanged({ provider: target.provider ?? "", model: model.trim() });
      }
      if (keyChanged) {
        const rotated = await rotateSecretAction(workspaceId, target.secret_ref, apiKey.trim());
        if (!rotated.ok) {
          setError(presentErrorString({ errorCode: rotated.errorCode, message: rotated.error, action: ROTATE_ACTION }));
          return;
        }
        captureKeyRotated(target.provider ?? "");
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
        <DialogDescription>Change the model, or enter a new key to rotate the shared credential.</DialogDescription>
      </DialogHeader>
      <div className="space-y-3">
        <ProviderModelSelect id={`${uid}-model`} provider={target.provider} model={model} onModelChange={setModel} />
        <div className="space-y-2">
          <Label htmlFor={`${uid}-key`}>New API key</Label>
          <Input
            id={`${uid}-key`}
            type="password"
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            placeholder="Leave blank to keep the current key"
            spellCheck={false}
            autoComplete="off"
            className="font-mono"
          />
          <p className="text-xs text-muted-foreground">
            Rotating updates every entry sharing this key, not just this row.
          </p>
        </div>
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
          <EditForm key={target.id} workspaceId={workspaceId} target={target} onOpenChange={onOpenChange} onSaved={onSaved} />
        ) : null}
      </DialogContent>
    </Dialog>
  );
}
