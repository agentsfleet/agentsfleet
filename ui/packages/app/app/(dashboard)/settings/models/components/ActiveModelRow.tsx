"use client";

import { useState } from "react";
import { Alert, Button, DashboardRow, MetaGrid, StatusPill } from "@agentsfleet/design-system";
import { CpuIcon, ServerIcon } from "lucide-react";
import { resetProviderAction } from "../actions";
import { captureProviderReset } from "../lib/track";
import { useProviderAction } from "../lib/use-provider-action";
import { providerLabel } from "@/lib/api/model_caps";
import { SECRET_KIND, type Secret } from "@/lib/api/secrets";
import { PROVIDER_MODE, type TenantProvider } from "@/lib/types";
import HeroChangeModelPanel from "./HeroChangeModelPanel";
import HeroReplaceKeyPanel from "./HeroReplaceKeyPanel";
import ProviderKeyForm from "./ProviderKeyForm";

type Props = {
  workspaceId: string;
  provider: TenantProvider | null;
  secrets: Secret[];
};

const PANEL = { idle: "idle", changeModel: "changeModel", replaceKey: "replaceKey", addKey: "addKey" } as const;
type Panel = (typeof PANEL)[keyof typeof PANEL];

const BILLING_PROVIDER_DIRECT = "Provider direct";
const BILLING_TENANT_BALANCE = "Tenant balance";
const MANAGED_PROVIDER = "agentsfleet managed";

// Shared with ProviderSwitchList — the same add-a-key affordance, named once.
export const ADD_KEY_AND_MODEL_LABEL = "Add key & model";
const RESET_ACTION = "switch to platform defaults";

// Threshold + divisor for the "k" context abbreviation (200000 → "200k").
const TOKENS_PER_K = 1000;

/** 200000 → "200k"; smaller / unknown values render as-is or "default". */
function formatContext(tokens: number | undefined): string {
  if (!tokens || tokens <= 0) return "default";
  return tokens >= TOKENS_PER_K ? `${Math.round(tokens / TOKENS_PER_K)}k` : String(tokens);
}

export default function ActiveModelRow({ workspaceId, provider, secrets }: Props) {
  const [panel, setPanel] = useState<Panel>(PANEL.idle);
  const { pending, error, run } = useProviderAction();

  const live = provider?.mode === PROVIDER_MODE.self_managed;
  const secretRef = live ? provider.secret_ref : null;
  const activeSecret = secrets.find((c) => c.name === secretRef) ?? null;
  // A custom secret can't be rotated as a model key; only provider-key/custom-endpoint secrets show Replace key.
  const canRotate =
    activeSecret?.kind === SECRET_KIND.provider_key || activeSecret?.kind === SECRET_KIND.custom_endpoint;

  function onReset(fromProvider: string) {
    void run(RESET_ACTION, async () => {
      const res = await resetProviderAction();
      if (!res.ok) return { message: res.error, errorCode: res.errorCode };
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
            via <span className="font-mono">{secretRef ?? provider.provider}</span>
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

          {live && secretRef && panel === PANEL.changeModel ? (
            <HeroChangeModelPanel
              provider={provider.provider}
              secretRef={secretRef}
              onClose={() => setPanel(PANEL.idle)}
            />
          ) : null}

          {live && secretRef && panel === PANEL.replaceKey ? (
            <HeroReplaceKeyPanel
              workspaceId={workspaceId}
              secretRef={secretRef}
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
