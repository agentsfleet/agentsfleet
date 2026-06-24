"use client";

import { useState, type KeyboardEvent } from "react";
import { useRouter } from "next/navigation";
import {
  Alert,
  Button,
  Input,
  Label,
  Spinner,
} from "@agentsfleet/design-system";
import { createCredentialAction } from "../actions";
import { presentErrorString } from "@/lib/errors";
import type { CredentialData } from "@/lib/api/credentials";
import {
  OPENAI_COMPATIBLE_PROVIDER,
  CREDENTIAL_FIELD,
  HTTPS_SCHEME_PREFIX,
} from "@/lib/types";
import { EVENTS } from "@/lib/analytics/events";
import { captureProductEvent } from "@/lib/analytics/posthog";

export type CustomEndpointFormProps = {
  workspaceId: string;
  /** Optional: fired with the stored credential name (e.g. parent auto-select). */
  onCreated?: (name: string) => void;
};

export const BASE_URL_NOT_HTTPS =
  "Base URL must use https:// — a custom endpoint is rejected otherwise.";

// Client-side https gate, matching the CLI option validator and the server-side
// guard's first check. Parses as a URL so a malformed value is caught for the
// same reason rather than slipping through a bare prefix test. The server
// re-validates and additionally blocks SSRF-unsafe hosts (loopback / private /
// metadata) — this is only the cheap, name-the-reason inline check.
export function isHttpsUrl(value: string): boolean {
  const trimmed = value.trim();
  if (!trimmed.startsWith(HTTPS_SCHEME_PREFIX)) return false;
  try {
    return new URL(trimmed).protocol === "https:";
  } catch {
    return false;
  }
}

/**
 * Add a custom OpenAI-compatible model-provider credential to the vault: a base
 * URL (required, https), a default model (required), and an optional API key.
 * Submits the credential JSON `{ provider: "openai-compatible", base_url, model,
 * api_key? }` via createCredential — the `base_url` + `model` ride in the saved
 * credential (no schema change). The resolver requires `model` to activate the
 * credential, so it is collected here rather than left for later. A non-https
 * URL is flagged inline (the "not-https" reason) before any request; the server
 * enforces the full SSRF guard and returns a typed error for a blocked host.
 */
export default function CustomEndpointForm({ workspaceId, onCreated }: CustomEndpointFormProps) {
  const router = useRouter();
  const [name, setName] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [model, setModel] = useState("");
  const [apiKey, setApiKey] = useState("");
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
      const data: CredentialData = {
        [CREDENTIAL_FIELD.provider]: OPENAI_COMPATIBLE_PROVIDER,
        [CREDENTIAL_FIELD.baseUrl]: baseUrl.trim(),
        [CREDENTIAL_FIELD.model]: model.trim(),
      };
      const key = apiKey.trim();
      if (key !== "") data[CREDENTIAL_FIELD.apiKey] = key;

      const result = await createCredentialAction(workspaceId, { name: name.trim(), data });
      if (!result.ok) {
        setError(
          presentErrorString({
            errorCode: result.errorCode,
            message: result.error,
            action: "store the custom endpoint",
          }),
        );
        return;
      }
      captureProductEvent(EVENTS.credential_added, { credential_name: name.trim() });
      onCreated?.(name.trim());
      setName("");
      setBaseUrl("");
      setModel("");
      setApiKey("");
      router.refresh();
    } finally {
      setPending(false);
    }
  }

  // Enter on any field stores the endpoint here (the inputs are standalone, not
  // wrapped in a native form element).
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
      </div>
      <div className="space-y-2">
        <Label htmlFor="custom-endpoint-model">Model</Label>
        <Input
          id="custom-endpoint-model"
          value={model}
          onChange={(e) => setModel(e.target.value)}
          onKeyDown={onFieldKeyDown}
          placeholder="claude-opus-4-8"
          className="font-mono text-sm"
          spellCheck={false}
          autoComplete="off"
        />
        <p className="text-xs text-muted-foreground">
          The model id this endpoint serves — required to activate the credential.
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
      {error ? (
        <Alert variant="destructive" className="text-xs">
          {error}
        </Alert>
      ) : null}
      <Button type="button" onClick={() => void submit()} disabled={pending || !canSubmit}>
        {pending ? <Spinner size="sm" srLabel="Storing" /> : null}
        Add custom endpoint
      </Button>
    </div>
  );
}
