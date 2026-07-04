"use client";

import { useState } from "react";
import { Alert, Button, DashboardRow, MetaGrid, StatusPill } from "@agentsfleet/design-system";
import { CpuIcon, ServerIcon } from "lucide-react";
import { resetProviderAction } from "../actions";
import { captureProviderReset } from "../lib/track";
import { useProviderAction } from "../lib/use-provider-action";
import { providerLabel } from "@/lib/api/model_caps";
import { CREDENTIAL_KIND, type Credential } from "@/lib/api/credentials";
import { PROVIDER_MODE, type TenantProvider } from "@/lib/types";
import HeroChangeModelPanel from "./HeroChangeModelPanel";
import HeroReplaceKeyPanel from "./HeroReplaceKeyPanel";
import ProviderKeyForm from "./ProviderKeyForm";

type Props = {
  workspaceId: string;
  provider: TenantProvider | null;
  credentials: Credential[];
};

const PANEL = { idle: "idle", changeModel: "changeModel", replaceKey: "replaceKey", addKey: "addKey" } as const;
type Panel = (typeof PANEL)[keyof typeof PANEL];

const BILLING_PROVIDER_DIRECT = "Provider direct";
const BILLING_TENANT_BALANCE = "Tenant balance";
const MANAGED_PROVIDER = "agentsfleet managed";

// Shared with ProviderSwitchList — the same add-a-key affordance, named once.
export const ADD_KEY_AND_MODEL_LABEL = "Add key & model";

// Threshold + divisor for the "k" context abbreviation (200000 → "200k").
const TOKENS_PER_K = 1000;

/** 200000 → "200k"; smaller / unknown values render as-is or "default". */
function formatContext(tokens: number | undefined): string {
  if (!tokens || tokens <= 0) return "default";
  return tokens >= TOKENS_PER_K ? `${Math.round(tokens / TOKENS_PER_K)}k` : String(tokens);
}

export default function ActiveModelHero({ workspaceId, provider, credentials }: Props) {
  const [panel, setPanel] = useState<Panel>(PANEL.idle);
  const { pending, error, run } = useProviderAction();

  const live = provider?.mode === PROVIDER_MODE.self_managed;
  const credRef = live ? provider.credential_ref : null;
  const activeCred = credentials.find((c) => c.name === credRef) ?? null;
  // A custom secret can't be rotated as a model key; only real model credentials show Replace key.
  const canRotate =
    activeCred?.kind === CREDENTIAL_KIND.provider_key || activeCred?.kind === CREDENTIAL_KIND.custom_endpoint;

  function onReset(fromProvider: string) {
    void run(async () => {
      const res = await resetProviderAction();
      if (!res.ok) return res.error;
      captureProviderReset(fromProvider);
      return null;
    });
  }

  const metaItems = live
    ? [
        { label: "Provider", value: providerLabel(provider.provider) },
        { label: "Context", value: formatContext(provider.context_cap_tokens) },
        { label: "Billing", value: BILLING_PROVIDER_DIRECT },
      ]
    : [
        { label: "Provider", value: MANAGED_PROVIDER },
        { label: "Context", value: "default" },
        { label: "Billing", value: BILLING_TENANT_BALANCE },
      ];

  return (
    <DashboardRow
      data-testid="active-model-hero"
      data-live={live}
      icon={live ? <CpuIcon size={15} /> : <ServerIcon size={15} />}
      title={live ? provider.model : "Platform default model"}
      description={
        live ? (
          <>
            via <span className="font-mono">{credRef ?? provider.provider}</span>
          </>
        ) : null
      }
      action={
        <StatusPill variant={live ? "pulse" : "neutral"} dot>
          {live ? "LIVE" : "DEFAULT"}
        </StatusPill>
      }
      meta={
        <div className="space-y-md">
          <MetaGrid bordered items={metaItems} />

          {live ? (
            <div className="flex flex-wrap items-center gap-md">
              <Button
                type="button"
                variant="outline"
                disabled={pending}
                aria-expanded={panel === PANEL.changeModel}
                onClick={() => setPanel((p) => (p === PANEL.changeModel ? PANEL.idle : PANEL.changeModel))}
              >
                Change model
              </Button>
              {canRotate ? (
                <Button
                  type="button"
                  variant="outline"
                  disabled={pending}
                  aria-expanded={panel === PANEL.replaceKey}
                  onClick={() => setPanel((p) => (p === PANEL.replaceKey ? PANEL.idle : PANEL.replaceKey))}
                >
                  Replace key
                </Button>
              ) : null}
              <Button type="button" variant="ghost" disabled={pending} onClick={() => onReset(provider.provider)}>
                Switch to platform defaults
              </Button>
            </div>
          ) : (
            <div>
              <Button
                type="button"
                size="sm"
                disabled={pending}
                aria-expanded={panel === PANEL.addKey}
                onClick={() => setPanel((p) => (p === PANEL.addKey ? PANEL.idle : PANEL.addKey))}
              >
                {ADD_KEY_AND_MODEL_LABEL}
              </Button>
            </div>
          )}

          {live && credRef && panel === PANEL.changeModel ? (
            <HeroChangeModelPanel
              provider={provider.provider}
              credentialRef={credRef}
              onClose={() => setPanel(PANEL.idle)}
            />
          ) : null}

          {live && credRef && panel === PANEL.replaceKey ? (
            <HeroReplaceKeyPanel
              workspaceId={workspaceId}
              credentialRef={credRef}
              provider={provider.provider}
              currentModel={provider.model}
              onClose={() => setPanel(PANEL.idle)}
            />
          ) : null}

          {!live && panel === PANEL.addKey ? (
            <ProviderKeyForm
              workspaceId={workspaceId}
              activate
              onDone={() => setPanel(PANEL.idle)}
              onCancel={() => setPanel(PANEL.idle)}
            />
          ) : null}

          {error ? (
            <Alert variant="destructive" className="text-xs">
              {error}
            </Alert>
          ) : null}
        </div>
      }
    />
  );
}
