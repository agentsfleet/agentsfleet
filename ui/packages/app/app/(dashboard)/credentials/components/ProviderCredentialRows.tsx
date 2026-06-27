"use client";

import { useState, useTransition } from "react";
import { useRouter } from "next/navigation";
import {
  Alert,
  Button,
  DashboardRow,
  Input,
  Spinner,
  StatusPill,
} from "@agentsfleet/design-system";
import { KeyRoundIcon } from "lucide-react";
import { createCredentialAction } from "../actions";
import { presentErrorString } from "@/lib/errors";
import {
  CREDENTIAL_FIELD,
  PROVIDER_MODE,
  type TenantProvider,
} from "@/lib/types";
import { EVENTS } from "@/lib/analytics/events";
import { captureProductEvent } from "@/lib/analytics/posthog";

const CONNECTED = "Connected";
const NOT_CONNECTED = "Not connected";
const ADD_KEY_LABEL = "Add key";
const REPLACE_LABEL = "Replace";
const SAVE_LABEL = "Save";
const REPLACE_KEY_LABEL = "Replace key";
const API_KEY_REQUIRED = "API key is required";
const STORE_ACTION = "store the provider key";
const WRITE_ONLY_HINT = "Write-only. Replace to rotate. Pick the model in Models.";
const PROVIDER_ID = {
  anthropic: "anthropic",
  openai: "openai",
} as const;

// Provider rows store ONLY the raw key (preview parity): the credential carries
// `provider` + `api_key`; the model is chosen later in Models → own-key setup
// (which writes the tenant provider's `model` + `credential_ref`). Storing a
// model on the credential here would duplicate that and the vault never reads it
// back, so it is deliberately omitted.
const PROVIDER_ROWS = [
  {
    id: PROVIDER_ID.anthropic,
    title: "Anthropic API key",
    description: "Use an Anthropic key for Claude models.",
    keyPlaceholder: "sk-ant-...",
    defaultCredential: PROVIDER_ID.anthropic,
  },
  {
    id: PROVIDER_ID.openai,
    title: "OpenAI API key",
    description: "Use an OpenAI key for GPT models.",
    keyPlaceholder: "sk-...",
    defaultCredential: PROVIDER_ID.openai,
  },
] as const;

type ProviderRow = (typeof PROVIDER_ROWS)[number];

type ProviderCredentialRowsProps = {
  workspaceId: string;
  provider: TenantProvider | null;
};

function providerMatchesCredential(providerId: string, provider: TenantProvider | null) {
  if (!provider || provider.mode !== PROVIDER_MODE.self_managed) return false;
  const providerName = provider.provider.toLowerCase();
  const credentialName = provider.credential_ref?.toLowerCase() ?? "";
  return providerName.includes(providerId) || credentialName.includes(providerId);
}

// Connected rows keep replacing the credential the active model already points
// at; a fresh row stores under the provider's default name.
function credentialNameFor(row: ProviderRow, provider: TenantProvider | null) {
  return providerMatchesCredential(row.id, provider) && provider?.credential_ref
    ? provider.credential_ref
    : row.defaultCredential;
}

function ProviderCredentialForm({
  row,
  workspaceId,
  provider,
  onSaved,
}: {
  row: ProviderRow;
  workspaceId: string;
  provider: TenantProvider | null;
  onSaved: () => void;
}) {
  const router = useRouter();
  const [pending, startTransition] = useTransition();
  const [apiKey, setApiKey] = useState("");
  const [error, setError] = useState<string | null>(null);
  const connected = providerMatchesCredential(row.id, provider);

  function save() {
    const credentialApiKey = apiKey.trim();
    setError(null);
    if (credentialApiKey === "") {
      setError(API_KEY_REQUIRED);
      return;
    }

    const name = credentialNameFor(row, provider);
    startTransition(async () => {
      const result = await createCredentialAction(workspaceId, {
        name,
        data: {
          [CREDENTIAL_FIELD.provider]: row.id,
          [CREDENTIAL_FIELD.apiKey]: credentialApiKey,
        },
      });
      if (!result.ok) {
        setError(
          presentErrorString({
            errorCode: result.errorCode,
            message: result.error,
            action: STORE_ACTION,
          }),
        );
        return;
      }
      captureProductEvent(EVENTS.credential_added, { credential_name: name });
      setApiKey("");
      onSaved();
      router.refresh();
    });
  }

  return (
    <div className="space-y-md border-t border-border bg-surface-deep px-lg py-md">
      <div className="flex flex-col gap-md sm:flex-row sm:items-center">
        <Input
          aria-label={`${row.title} value`}
          type="password"
          value={apiKey}
          onChange={(event) => setApiKey(event.target.value)}
          placeholder={row.keyPlaceholder}
          spellCheck={false}
          autoComplete="off"
          className="font-mono sm:flex-1"
        />
        <Button type="button" onClick={save} disabled={pending}>
          {pending ? <Spinner size="sm" srLabel="Saving" /> : null}
          {connected ? REPLACE_KEY_LABEL : SAVE_LABEL}
        </Button>
      </div>
      <p className="text-body-sm leading-body-sm text-muted-foreground">{WRITE_ONLY_HINT}</p>
      {error ? (
        <Alert variant="destructive" className="text-xs">
          {error}
        </Alert>
      ) : null}
    </div>
  );
}

export default function ProviderCredentialRows({
  workspaceId,
  provider,
}: ProviderCredentialRowsProps) {
  const [openProvider, setOpenProvider] = useState<string | null>(null);
  return (
    <>
      {PROVIDER_ROWS.map((row) => {
        const connected = providerMatchesCredential(row.id, provider);
        const open = openProvider === row.id;
        return (
          <div key={row.id} className="border-b border-border last:border-b-0">
            <DashboardRow
              icon={<KeyRoundIcon size={15} />}
              title={row.title}
              description={
                connected && provider?.credential_ref ? (
                  <>
                    <code className="font-mono">{provider.credential_ref}</code> is selected in
                    Models. Write-only.
                  </>
                ) : (
                  row.description
                )
              }
              action={
                <div className="flex items-center gap-2">
                  <StatusPill variant={connected ? "success" : "neutral"} dot={connected}>
                    {connected ? CONNECTED : NOT_CONNECTED}
                  </StatusPill>
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    aria-expanded={open}
                    onClick={() => setOpenProvider(open ? null : row.id)}
                  >
                    {connected ? REPLACE_LABEL : ADD_KEY_LABEL}
                  </Button>
                </div>
              }
            />
            {open ? (
              <ProviderCredentialForm
                row={row}
                provider={provider}
                workspaceId={workspaceId}
                onSaved={() => setOpenProvider(null)}
              />
            ) : null}
          </div>
        );
      })}
    </>
  );
}
