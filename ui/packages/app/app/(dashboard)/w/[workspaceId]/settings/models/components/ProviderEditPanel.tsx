"use client";

import { useState } from "react";
import { Alert, Button, DashboardPanel, Input, Label, Spinner } from "@agentsfleet/design-system";
import { rotateSecretAction, setProviderSelfManagedAction } from "../actions";
import { captureKeyRotated, captureModelChanged } from "../lib/track";
import { useProviderAction } from "../lib/use-provider-action";
import ProviderModelSelect from "./ProviderModelSelect";

type Props = {
  workspaceId: string;
  /** Active provider id — scopes the model picker and tags the key_rotated event. */
  provider: string;
  /** The active secret being edited; rotating the key keeps this same ref. */
  secretRef: string;
  currentModel: string;
  onClose: () => void;
};

const EDIT_ACTION = "update the key and model";

/** One combined edit surface for an active row: replace the key, change the model, or both. */
export default function ProviderEditPanel({ workspaceId, provider, secretRef, currentModel, onClose }: Props) {
  const [apiKey, setApiKey] = useState("");
  const [model, setModel] = useState(currentModel);
  const { pending, error, run } = useProviderAction();

  const keyChanged = apiKey.trim() !== "";
  const modelChanged = model.trim() !== "" && model.trim() !== currentModel;
  const canSubmit = model.trim() !== "" && (keyChanged || modelChanged);

  function save() {
    if (!canSubmit) return;
    void run(
      EDIT_ACTION,
      async () => {
        if (keyChanged) {
          const rotated = await rotateSecretAction(workspaceId, secretRef, apiKey.trim());
          if (!rotated.ok) return { message: rotated.error, errorCode: rotated.errorCode };
          captureKeyRotated(provider);
        }
        if (modelChanged) {
          const res = await setProviderSelfManagedAction({ secret_ref: secretRef, model: model.trim() });
          if (!res.ok) return { message: res.error, errorCode: res.errorCode };
          captureModelChanged(res.data);
        }
        return null;
      },
      onClose,
    );
  }

  return (
    <DashboardPanel className="space-y-3" data-testid="provider-edit-panel">
      <div className="space-y-2">
        <Label htmlFor="provider-edit-api-key">New API key</Label>
        <Input
          id="provider-edit-api-key"
          type="password"
          value={apiKey}
          onChange={(e) => setApiKey(e.target.value)}
          placeholder="Leave blank to keep the current key"
          spellCheck={false}
          autoComplete="off"
          className="font-mono"
        />
      </div>
      <ProviderModelSelect id="provider-edit-model" provider={provider} model={model} onModelChange={setModel} />
      <div className="flex flex-wrap gap-md">
        <Button type="button" onClick={save} disabled={pending || !canSubmit}>
          {pending ? <Spinner size="sm" srLabel="Saving" /> : null}
          Save
        </Button>
        <Button type="button" variant="outline" disabled={pending} onClick={onClose}>
          Cancel
        </Button>
      </div>
      {error ? (
        <Alert variant="destructive" className="text-xs">
          {error}
        </Alert>
      ) : null}
    </DashboardPanel>
  );
}
