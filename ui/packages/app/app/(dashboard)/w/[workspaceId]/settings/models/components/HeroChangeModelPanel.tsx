"use client";

import { useState } from "react";
import { Alert, Button, DashboardPanel, Spinner } from "@agentsfleet/design-system";
import { setProviderSelfManagedAction } from "../actions";
import { captureModelChanged } from "../lib/track";
import { useProviderAction } from "../lib/use-provider-action";
import ProviderModelSelect from "./ProviderModelSelect";

type Props = {
  /** Provider id of the active secret — scopes the model picker. */
  provider: string;
  /** The active secret's ref; the model is re-pointed against the same key. */
  secretRef: string;
  onClose: () => void;
};

const CHANGE_MODEL_ACTION = "change the model";

/** Hero "Change model" — same key, a different model from this provider's catalogue. */
export default function HeroChangeModelPanel({ provider, secretRef, onClose }: Props) {
  const [model, setModel] = useState("");
  const { pending, error, run } = useProviderAction();

  function save() {
    if (model.trim() === "") return;
    void run(
      CHANGE_MODEL_ACTION,
      async () => {
        const res = await setProviderSelfManagedAction({ secret_ref: secretRef, model: model.trim() });
        if (!res.ok) return { message: res.error, errorCode: res.errorCode };
        captureModelChanged(res.data);
        return null;
      },
      onClose,
    );
  }

  return (
    <DashboardPanel className="space-y-3" data-testid="hero-change-model">
      <ProviderModelSelect
        id="hero-change-model-select"
        provider={provider}
        model={model}
        onModelChange={setModel}
        label="Change model"
      />
      <div className="flex flex-wrap gap-md">
        <Button type="button" onClick={save} disabled={pending || model.trim() === ""}>
          {pending ? <Spinner size="sm" srLabel="Saving" /> : null}
          Save model
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
