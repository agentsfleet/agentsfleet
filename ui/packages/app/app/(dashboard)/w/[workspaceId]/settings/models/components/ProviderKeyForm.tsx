"use client";

import { useId, useState, type KeyboardEvent } from "react";
import { useRouter } from "next/navigation";
import {
  Alert,
  Button,
  Input,
  Label,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Spinner,
} from "@agentsfleet/design-system";
import { createSecretAction } from "@/app/(dashboard)/w/[workspaceId]/secrets/actions";
import { setProviderSelfManagedAction } from "../actions";
import { detectProviderFromKey } from "../lib/detect-provider";
import { presentErrorString } from "@/lib/errors";
import { SECRET_FIELD } from "@/lib/types";
import { EVENTS } from "@/lib/analytics/events";
import { captureProductEvent } from "@/lib/analytics/posthog";
import { captureModelActivated } from "../lib/track";
import { providerLabel, uniqueProviders } from "@/lib/api/model_caps";
import { useModelCatalogue } from "./ModelCatalogueProvider";
import ProviderModelSelect from "./ProviderModelSelect";

export type ProviderKeyFormProps = {
  workspaceId: string;
  /** Lock the form to one provider (switch-list add); omit for the generic paste-detect add. */
  provider?: string;
  /** When true, the stored secret is activated as the tenant provider on save. */
  activate?: boolean;
  onDone: () => void;
  onCancel?: () => void;
};

const STORE_ACTION = "store the provider key";
const ACTIVATE_ACTION = "activate this model";

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
  const { models } = useModelCatalogue();
  const providerOptions = uniqueProviders(models);
  const uid = useId();
  const apiKeyFieldId = `${uid}-api-key`;
  const providerFieldId = `${uid}-provider`;
  const modelFieldId = `${uid}-model`;
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
      const created = await createSecretAction(workspaceId, {
        name,
        data: {
          [SECRET_FIELD.provider]: name,
          [SECRET_FIELD.apiKey]: apiKey.trim(),
          [SECRET_FIELD.model]: model.trim(),
        },
      });
      if (!created.ok) {
        setError(
          presentErrorString({ errorCode: created.errorCode, message: created.error, action: STORE_ACTION }),
        );
        return;
      }
      captureProductEvent(EVENTS.secret_added, { secret_name: name });

      if (activate) {
        const set = await setProviderSelfManagedAction({ secret_ref: name, model: model.trim() });
        if (!set.ok) {
          setError(presentErrorString({ errorCode: set.errorCode, message: set.error, action: ACTIVATE_ACTION }));
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
        <Label htmlFor={apiKeyFieldId}>API key</Label>
        <Input
          id={apiKeyFieldId}
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
          <Label htmlFor={providerFieldId}>Provider</Label>
          {providerOptions.length > 0 ? (
            <Select
              value={provider}
              onValueChange={(value) => {
                setProvider(value);
                setModel("");
              }}
            >
              <SelectTrigger id={providerFieldId} aria-label="Provider">
                <SelectValue placeholder="Select a provider" />
              </SelectTrigger>
              <SelectContent>
                {providerOptions.map((p) => (
                  <SelectItem key={p} value={p}>
                    {providerLabel(p)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          ) : (
            <Input
              id={providerFieldId}
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
          )}
        </div>
      )}
      <ProviderModelSelect id={modelFieldId} provider={name || undefined} model={model} onModelChange={setModel} />
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
