"use client";

import { useState, type KeyboardEvent } from "react";
import { useRouter } from "next/navigation";
import { Alert, Button, Input, Label, Spinner } from "@agentsfleet/design-system";
import { createCredentialAction } from "@/app/(dashboard)/credentials/actions";
import { setProviderSelfManagedAction } from "../actions";
import { detectProviderFromKey } from "../lib/detect-provider";
import { presentErrorString } from "@/lib/errors";
import { CREDENTIAL_FIELD } from "@/lib/types";
import { EVENTS } from "@/lib/analytics/events";
import { captureProductEvent } from "@/lib/analytics/posthog";
import { captureModelActivated } from "../lib/track";
import ProviderModelSelect from "./ProviderModelSelect";

export type ProviderKeyFormProps = {
  workspaceId: string;
  /** Lock the form to one provider (switch-list add); omit for the generic paste-detect add. */
  provider?: string;
  /** When true, the stored credential is activated as the tenant provider on save. */
  activate?: boolean;
  onDone: () => void;
  onCancel?: () => void;
};

const STORE_ACTION = "store the provider key";

/**
 * Consolidated "add a provider key" form (supersedes InlineProviderKeyCreate +
 * the option-card own-key path). Stores `{ provider, api_key, model }` under the
 * provider slug, then — when `activate` — points the tenant provider at it in the
 * same flow. Locked-provider mode (a switch-list row) hides the provider field;
 * generic mode shows it and fills it from a pasted key's prefix
 * (detect-provider.ts). The model is a provider-scoped catalogue picker.
 */
export default function ProviderKeyForm({
  workspaceId,
  provider: lockedProvider,
  activate = false,
  onDone,
  onCancel,
}: ProviderKeyFormProps) {
  const router = useRouter();
  const locked = lockedProvider !== undefined;
  const [provider, setProvider] = useState(lockedProvider ?? "");
  const [apiKey, setApiKey] = useState("");
  const [model, setModel] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);

  const name = provider.trim();
  const canSubmit = name !== "" && apiKey.trim() !== "" && model.trim() !== "";

  function onApiKeyChange(value: string) {
    setApiKey(value);
    // Generic mode only: a key's prefix maps to a provider (detect-provider.ts).
    if (locked) return;
    const detected = detectProviderFromKey(value);
    if (detected && detected !== provider) {
      setProvider(detected);
      setModel("");
    }
  }

  async function submit() {
    if (!canSubmit || pending) return;
    setError(null);
    setPending(true);
    try {
      const created = await createCredentialAction(workspaceId, {
        name,
        data: {
          [CREDENTIAL_FIELD.provider]: name,
          [CREDENTIAL_FIELD.apiKey]: apiKey.trim(),
          [CREDENTIAL_FIELD.model]: model.trim(),
        },
      });
      if (!created.ok) {
        setError(
          presentErrorString({ errorCode: created.errorCode, message: created.error, action: STORE_ACTION }),
        );
        return;
      }
      captureProductEvent(EVENTS.credential_added, { credential_name: name });

      if (activate) {
        const set = await setProviderSelfManagedAction({ credential_ref: name, model: model.trim() });
        if (!set.ok) {
          setError(set.error);
          return;
        }
        captureModelActivated(set.data);
      }
      onDone();
      router.refresh();
    } finally {
      setPending(false);
    }
  }

  function onFieldKeyDown(e: KeyboardEvent<HTMLInputElement>) {
    if (e.key === "Enter") {
      e.preventDefault();
      void submit();
    }
  }

  return (
    <div className="space-y-3" data-testid="provider-key-form">
      <div className="space-y-2">
        <Label htmlFor="provider-key-api-key">API key</Label>
        <Input
          id="provider-key-api-key"
          type="password"
          value={apiKey}
          onChange={(e) => onApiKeyChange(e.target.value)}
          onKeyDown={onFieldKeyDown}
          placeholder={locked ? "sk-..." : "paste your key — we'll detect common providers"}
          spellCheck={false}
          autoComplete="off"
        />
      </div>
      {locked ? null : (
        <div className="space-y-2">
          <Label htmlFor="provider-key-provider">Provider</Label>
          <Input
            id="provider-key-provider"
            value={provider}
            onChange={(e) => {
              setProvider(e.target.value);
              setModel("");
            }}
            onKeyDown={onFieldKeyDown}
            placeholder="anthropic"
            spellCheck={false}
            autoComplete="off"
          />
        </div>
      )}
      <ProviderModelSelect id="provider-key-model" provider={name || undefined} model={model} onModelChange={setModel} />
      {error ? (
        <Alert variant="destructive" className="text-xs">
          {error}
        </Alert>
      ) : null}
      <div className="flex flex-wrap gap-md">
        <Button type="button" onClick={() => void submit()} disabled={pending || !canSubmit}>
          {pending ? <Spinner size="sm" srLabel="Saving" /> : null}
          {activate ? "Save & make active" : "Save key"}
        </Button>
        {onCancel ? (
          <Button type="button" variant="outline" disabled={pending} onClick={onCancel}>
            Cancel
          </Button>
        ) : null}
      </div>
    </div>
  );
}
