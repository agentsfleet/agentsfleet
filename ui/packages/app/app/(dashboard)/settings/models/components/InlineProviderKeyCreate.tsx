"use client";

import { useState, type KeyboardEvent } from "react";
import { useRouter } from "next/navigation";
import { Alert, Button, Input, Label, Spinner } from "@usezombie/design-system";
import { createCredentialAction } from "@/app/(dashboard)/credentials/actions";
import { presentErrorString } from "@/lib/errors";
import type { ModelCap } from "@/lib/api/model_caps";
import { detectProviderFromKey } from "../lib/detect-provider";

export type InlineProviderKeyCreateProps = {
  workspaceId: string;
  catalogue: ModelCap[];
  onCreated: (name: string) => void;
};

/**
 * Inline "add a provider key" form for the Models wizard — a purpose-built
 * structured form (provider / api_key / model), not the generic JSON-blob
 * AddCredentialForm. Pasting an API key fills the provider from its prefix
 * (client-side heuristic) and defaults the model from the catalogue; the
 * credential name tracks the provider slug. All three auto-fills yield to manual
 * edits. On success the parent selects the new credential; the API surfaces
 * duplicate-name and validation errors. Rendered inside the wizard's form
 * element, so it uses a button + onClick, not a nested form.
 */
export default function InlineProviderKeyCreate({
  workspaceId,
  catalogue,
  onCreated,
}: InlineProviderKeyCreateProps) {
  const router = useRouter();
  const [provider, setProvider] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [model, setModel] = useState("");
  const [name, setName] = useState("");
  const [nameEdited, setNameEdited] = useState(false);
  const [providerEdited, setProviderEdited] = useState(false);
  const [modelEdited, setModelEdited] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);

  // The credential name tracks the provider slug until the user types their own.
  const effectiveName = (nameEdited ? name : provider).trim();
  const canSubmit =
    provider.trim() !== "" && apiKey.trim() !== "" && model.trim() !== "" && effectiveName !== "";

  // When the provider becomes known, default the model to the first catalogue
  // entry for it — unless the user has already picked a model.
  function defaultModelFor(prov: string) {
    if (modelEdited) return;
    const match = catalogue.find((m) => m.provider === prov);
    if (match) setModel(match.id);
  }

  function onApiKeyChange(value: string) {
    setApiKey(value);
    // Paste-to-fill: a key's prefix maps to a provider (client-side heuristic,
    // detect-provider.ts). Yields once the user has typed their own provider.
    if (providerEdited) return;
    const detected = detectProviderFromKey(value);
    if (detected) {
      setProvider(detected);
      defaultModelFor(detected);
    }
  }

  function onProviderChange(value: string) {
    setProviderEdited(true);
    setProvider(value);
    defaultModelFor(value);
  }

  async function submit() {
    if (!canSubmit || pending) return;
    setError(null);
    setPending(true);
    try {
      const result = await createCredentialAction(workspaceId, {
        name: effectiveName,
        data: { provider: provider.trim(), api_key: apiKey.trim(), model: model.trim() },
      });
      if (!result.ok) {
        setError(
          presentErrorString({
            errorCode: result.errorCode,
            message: result.error,
            action: "store the credential",
          }),
        );
        return;
      }
      onCreated(effectiveName);
      router.refresh();
    } catch (err) {
      // A thrown action (network partition, Server-Action machinery) must not
      // leave the button stuck disabled or fail silently — surface it.
      setError(
        presentErrorString({
          message: err instanceof Error ? err.message : "Unexpected error",
          action: "store the credential",
        }),
      );
    } finally {
      setPending(false);
    }
  }

  // The fields live inside the wizard's form; intercept Enter on each field so it
  // stores the key here instead of submitting the outer provider form.
  function onFieldKeyDown(e: KeyboardEvent<HTMLInputElement>) {
    if (e.key === "Enter") {
      e.preventDefault();
      void submit();
    }
  }

  return (
    <div className="space-y-3 rounded-md border border-dashed border-border p-3">
      <p className="text-xs font-medium text-muted-foreground">Add a new provider key</p>
      <div className="space-y-2">
        <Label htmlFor="inline-api-key">API key</Label>
        <Input
          id="inline-api-key"
          type="password"
          value={apiKey}
          onChange={(e) => onApiKeyChange(e.target.value)}
          onKeyDown={onFieldKeyDown}
          placeholder="paste your key — we'll detect the provider"
          spellCheck={false}
          autoComplete="off"
        />
      </div>
      <div className="space-y-2">
        <Label htmlFor="inline-provider">Provider</Label>
        <Input
          id="inline-provider"
          value={provider}
          onChange={(e) => onProviderChange(e.target.value)}
          onKeyDown={onFieldKeyDown}
          placeholder="anthropic"
          spellCheck={false}
          autoComplete="off"
        />
      </div>
      <div className="space-y-2">
        <Label htmlFor="inline-model">Model</Label>
        <Input
          id="inline-model"
          value={model}
          onChange={(e) => {
            setModelEdited(true);
            setModel(e.target.value);
          }}
          onKeyDown={onFieldKeyDown}
          placeholder="claude-sonnet-4-6"
          spellCheck={false}
          autoComplete="off"
        />
      </div>
      <div className="space-y-2">
        <Label htmlFor="inline-name">Credential name</Label>
        <Input
          id="inline-name"
          value={effectiveName}
          onChange={(e) => {
            setNameEdited(true);
            setName(e.target.value);
          }}
          onKeyDown={onFieldKeyDown}
          placeholder="defaults to the provider"
          spellCheck={false}
          autoComplete="off"
        />
      </div>
      {error ? (
        <Alert variant="destructive" className="text-xs">
          {error}
        </Alert>
      ) : null}
      <Button type="button" onClick={() => void submit()} disabled={pending || !canSubmit}>
        {pending ? <Spinner size="sm" srLabel="Storing" /> : null}
        Save key
      </Button>
    </div>
  );
}
