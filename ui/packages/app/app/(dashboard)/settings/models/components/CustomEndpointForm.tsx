"use client";

import { useState, type KeyboardEvent } from "react";
import { useRouter } from "next/navigation";
import { Alert, Button, Input, Label, Spinner } from "@agentsfleet/design-system";
import { createCredentialAction } from "@/app/(dashboard)/credentials/actions";
import { setProviderSelfManagedAction } from "../actions";
import { isHttpsUrl, BASE_URL_NOT_HTTPS } from "../lib/custom-endpoint";
import { presentErrorString } from "@/lib/errors";
import type { CredentialData } from "@/lib/api/credentials";
import { OPENAI_COMPATIBLE_PROVIDER, CREDENTIAL_FIELD } from "@/lib/types";
import { EVENTS } from "@/lib/analytics/events";
import { captureProductEvent } from "@/lib/analytics/posthog";
import { captureModelActivated } from "../lib/track";
import ProviderModelSelect from "./ProviderModelSelect";

export type CustomEndpointFormProps = {
  workspaceId: string;
  /** When true, the stored endpoint is activated as the tenant provider on save. */
  activate?: boolean;
  onDone: () => void;
  onCancel?: () => void;
};

const STORE_ACTION = "store the custom endpoint";

/**
 * Consolidated "add an OpenAI-compatible endpoint" form (supersedes the
 * credentials CustomEndpointForm + the own-key custom path). Stores
 * `{ provider: "openai-compatible", base_url, model, api_key? }` — the resolver
 * requires `model` to activate the credential, so it is collected here — then,
 * when `activate`, points the tenant provider at it. A non-https URL is flagged
 * inline before any request; the server enforces the full SSRF guard.
 */
export default function CustomEndpointForm({
  workspaceId,
  activate = false,
  onDone,
  onCancel,
}: CustomEndpointFormProps) {
  const router = useRouter();
  const [name, setName] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [model, setModel] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);

  const canSubmit = name.trim() !== "" && baseUrl.trim() !== "" && model.trim() !== "";

  async function submit() {
    if (!canSubmit || pending) return;
    setError(null);
    if (!isHttpsUrl(baseUrl)) {
      setError(BASE_URL_NOT_HTTPS);
      return;
    }
    setPending(true);
    try {
      const credName = name.trim();
      const credModel = model.trim();
      const data: CredentialData = {
        [CREDENTIAL_FIELD.provider]: OPENAI_COMPATIBLE_PROVIDER,
        [CREDENTIAL_FIELD.baseUrl]: baseUrl.trim(),
        [CREDENTIAL_FIELD.model]: credModel,
      };
      const key = apiKey.trim();
      if (key !== "") data[CREDENTIAL_FIELD.apiKey] = key;

      const created = await createCredentialAction(workspaceId, { name: credName, data });
      if (!created.ok) {
        setError(
          presentErrorString({ errorCode: created.errorCode, message: created.error, action: STORE_ACTION }),
        );
        return;
      }
      captureProductEvent(EVENTS.credential_added, { credential_name: credName });

      if (activate) {
        const set = await setProviderSelfManagedAction({ credential_ref: credName, model: credModel });
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
    <div className="space-y-3" data-testid="custom-endpoint-form">
      <div className="space-y-2">
        <Label htmlFor="custom-endpoint-name">Name</Label>
        <Input
          id="custom-endpoint-name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={onFieldKeyDown}
          placeholder="vllm-gateway"
          spellCheck={false}
          autoComplete="off"
        />
      </div>
      <div className="space-y-2">
        <Label htmlFor="custom-endpoint-base-url">Base URL</Label>
        <Input
          id="custom-endpoint-base-url"
          value={baseUrl}
          onChange={(e) => setBaseUrl(e.target.value)}
          onKeyDown={onFieldKeyDown}
          placeholder="https://vllm.corp/v1"
          spellCheck={false}
          autoComplete="off"
        />
        <p className="text-xs text-muted-foreground">
          Any OpenAI-compatible endpoint. Must use https; loopback and private hosts are rejected.
        </p>
      </div>
      <div className="space-y-2">
        <Label htmlFor="custom-endpoint-api-key">API key (optional)</Label>
        <Input
          id="custom-endpoint-api-key"
          type="password"
          value={apiKey}
          onChange={(e) => setApiKey(e.target.value)}
          onKeyDown={onFieldKeyDown}
          placeholder="leave blank if the endpoint needs no key"
          spellCheck={false}
          autoComplete="off"
        />
      </div>
      <ProviderModelSelect
        id="custom-endpoint-model"
        provider={OPENAI_COMPATIBLE_PROVIDER}
        model={model}
        onModelChange={setModel}
      />
      {error ? (
        <Alert variant="destructive" className="text-xs">
          {error}
        </Alert>
      ) : null}
      <div className="flex flex-wrap gap-md">
        <Button type="button" onClick={() => void submit()} disabled={pending || !canSubmit}>
          {pending ? <Spinner size="sm" srLabel="Saving" /> : null}
          {activate ? "Save & make active" : "Add custom endpoint"}
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
