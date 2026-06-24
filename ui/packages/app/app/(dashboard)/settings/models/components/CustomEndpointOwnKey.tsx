"use client";

import { useState, type KeyboardEvent } from "react";
import { Alert, Button, Input, Label, Spinner } from "@agentsfleet/design-system";
// Own-key sub-paths: pick a stored credential, or stand up a custom
// OpenAI-compatible endpoint (which reveals the base-URL field). Lives here with
// the custom path so ProviderSelector only composes it.
export const OWN_KEY_KIND = { stored: "stored", custom: "custom" } as const;
export type OwnKeyKind = (typeof OWN_KEY_KIND)[keyof typeof OWN_KEY_KIND];

export function OwnKeyKindToggle({
  kind,
  onKindChange,
}: {
  kind: OwnKeyKind;
  onKindChange: (kind: OwnKeyKind) => void;
}) {
  return (
    <fieldset className="flex flex-wrap gap-2 border-0 p-0">
      <legend className="sr-only">Own-key source</legend>
      <Button
        type="button"
        variant={kind === OWN_KEY_KIND.stored ? "default" : "outline"}
        size="sm"
        aria-pressed={kind === OWN_KEY_KIND.stored}
        onClick={() => onKindChange(OWN_KEY_KIND.stored)}
      >
        Stored credential
      </Button>
      <Button
        type="button"
        variant={kind === OWN_KEY_KIND.custom ? "default" : "outline"}
        size="sm"
        aria-pressed={kind === OWN_KEY_KIND.custom}
        onClick={() => onKindChange(OWN_KEY_KIND.custom)}
      >
        Custom — OpenAI-compatible
      </Button>
    </fieldset>
  );
}
import { createCredentialAction } from "@/app/(dashboard)/credentials/actions";
import {
  isHttpsUrl,
  BASE_URL_NOT_HTTPS,
} from "@/app/(dashboard)/credentials/components/CustomEndpointForm";
import { setProviderSelfManagedAction } from "../actions";
import { presentErrorString } from "@/lib/errors";
import type { CredentialData } from "@/lib/api/credentials";
import type { ModelCap } from "@/lib/api/model_caps";
import {
  OPENAI_COMPATIBLE_PROVIDER,
  CREDENTIAL_FIELD,
  type TenantProvider,
} from "@/lib/types";
import { EVENTS } from "@/lib/analytics/events";
import { captureProductEvent } from "@/lib/analytics/posthog";
import Step2Model from "./Step2Model";

export type CustomEndpointOwnKeyProps = {
  workspaceId: string;
  catalogue: ModelCap[];
  isPending: boolean;
  onSaved: (provider: TenantProvider) => void;
  onError: (message: string) => void;
};

/**
 * Own-key "Custom — OpenAI-compatible" path. Reveals a base-URL field, stores a
 * credential carrying `{ provider: "openai-compatible", base_url, model, api_key? }`,
 * then points the tenant provider at it (`setTenantProviderSelfManaged` with the
 * new credential's ref). The model collected here is written into the credential
 * (the resolver requires it to activate the credential) AND passed as the PUT
 * override, so the activation probe always has a model to read. A non-https URL
 * is flagged inline before any request; the server enforces the SSRF guard.
 */
export default function CustomEndpointOwnKey({
  workspaceId,
  catalogue,
  isPending,
  onSaved,
  onError,
}: CustomEndpointOwnKeyProps) {
  const [name, setName] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [model, setModel] = useState("");
  const [saving, setSaving] = useState(false);

  const busy = saving || isPending;
  const canSubmit = name.trim() !== "" && baseUrl.trim() !== "" && model.trim() !== "";

  async function submit() {
    if (!canSubmit || busy) return;
    if (!isHttpsUrl(baseUrl)) {
      onError(BASE_URL_NOT_HTTPS);
      return;
    }
    setSaving(true);
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
        onError(
          presentErrorString({
            errorCode: created.errorCode,
            message: created.error,
            action: "store the custom endpoint",
          }),
        );
        return;
      }
      captureProductEvent(EVENTS.credential_added, { credential_name: credName });

      const set = await setProviderSelfManagedAction({
        credential_ref: credName,
        model: credModel,
      });
      if (!set.ok) {
        onError(set.error);
        return;
      }
      onSaved(set.data);
    } finally {
      setSaving(false);
    }
  }

  // Enter on any field saves the custom endpoint (the inputs are standalone, not
  // wrapped in a native form element).
  function onFieldKeyDown(e: KeyboardEvent<HTMLInputElement>) {
    if (e.key === "Enter") {
      e.preventDefault();
      void submit();
    }
  }

  return (
    <div className="space-y-3" data-testid="custom-endpoint-own-key">
      <div className="space-y-2">
        <Label htmlFor="custom-own-key-base-url">Base URL</Label>
        <Input
          id="custom-own-key-base-url"
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
        <Label htmlFor="custom-own-key-api-key">API key (optional)</Label>
        <Input
          id="custom-own-key-api-key"
          type="password"
          value={apiKey}
          onChange={(e) => setApiKey(e.target.value)}
          onKeyDown={onFieldKeyDown}
          placeholder="leave blank if the endpoint needs no key"
          spellCheck={false}
          autoComplete="off"
        />
      </div>
      <div className="space-y-2">
        <Label htmlFor="custom-own-key-name">Credential name</Label>
        <Input
          id="custom-own-key-name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={onFieldKeyDown}
          placeholder="vllm-gateway"
          spellCheck={false}
          autoComplete="off"
        />
      </div>
      <Step2Model catalogue={catalogue} model={model} onModelChange={setModel} />
      <Alert variant="info" className="text-xs">
        The base URL and model are saved on this credential; pick the model this endpoint serves.
      </Alert>
      <Button type="button" onClick={() => void submit()} disabled={busy || !canSubmit}>
        {busy ? <Spinner size="sm" srLabel="Saving" /> : null}
        Save custom endpoint
      </Button>
    </div>
  );
}
