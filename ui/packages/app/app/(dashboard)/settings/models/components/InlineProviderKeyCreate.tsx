"use client";

import { useState, type KeyboardEvent } from "react";
import { useRouter } from "next/navigation";
import { Alert, Button, Input, Label, Spinner } from "@usezombie/design-system";
import { createCredentialAction } from "@/app/(dashboard)/credentials/actions";
import { presentErrorString } from "@/lib/errors";

export type InlineProviderKeyCreateProps = {
  workspaceId: string;
  onCreated: (name: string) => void;
};

/**
 * Inline "add a provider key" form for the Models wizard — a purpose-built
 * structured form (provider / api_key / model), not the generic JSON-blob
 * AddCredentialForm. The credential name defaults to the provider slug (editable)
 * so the common case is one decision. On success the parent selects the new
 * credential; the API surfaces duplicate-name and validation errors. Rendered
 * inside the wizard's form element, so it uses a button + onClick, not a nested form.
 */
export default function InlineProviderKeyCreate({
  workspaceId,
  onCreated,
}: InlineProviderKeyCreateProps) {
  const router = useRouter();
  const [provider, setProvider] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [model, setModel] = useState("");
  const [name, setName] = useState("");
  const [nameEdited, setNameEdited] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);

  // The credential name tracks the provider slug until the user types their own.
  const effectiveName = (nameEdited ? name : provider).trim();
  const canSubmit =
    provider.trim() !== "" && apiKey.trim() !== "" && model.trim() !== "" && effectiveName !== "";

  async function submit() {
    if (!canSubmit) return;
    setError(null);
    setPending(true);
    const result = await createCredentialAction(workspaceId, {
      name: effectiveName,
      data: { provider: provider.trim(), api_key: apiKey.trim(), model: model.trim() },
    });
    setPending(false);
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
        <Label htmlFor="inline-provider">Provider</Label>
        <Input
          id="inline-provider"
          value={provider}
          onChange={(e) => setProvider(e.target.value)}
          onKeyDown={onFieldKeyDown}
          placeholder="anthropic"
          spellCheck={false}
          autoComplete="off"
        />
      </div>
      <div className="space-y-2">
        <Label htmlFor="inline-api-key">API key</Label>
        <Input
          id="inline-api-key"
          type="password"
          value={apiKey}
          onChange={(e) => setApiKey(e.target.value)}
          onKeyDown={onFieldKeyDown}
          placeholder="sk-…"
          spellCheck={false}
          autoComplete="off"
        />
      </div>
      <div className="space-y-2">
        <Label htmlFor="inline-model">Model</Label>
        <Input
          id="inline-model"
          value={model}
          onChange={(e) => setModel(e.target.value)}
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
