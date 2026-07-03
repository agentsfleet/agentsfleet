"use client";

import { useState } from "react";
import { Alert, Button, DashboardPanel, EYEBROW_CLASS, MetaGrid, StatusPill } from "@agentsfleet/design-system";
import { cn } from "@/lib/utils";
import { resetProviderAction } from "../actions";
import { captureProviderReset } from "../lib/track";
import { useProviderAction } from "../lib/use-provider-action";
import { providerLabel } from "@/lib/api/model_caps";
import { CREDENTIAL_KIND, type Credential } from "@/lib/api/credentials";
import { PROVIDER_MODE, type TenantProvider } from "@/lib/types";
import HeroChangeModelPanel from "./HeroChangeModelPanel";
import HeroReplaceKeyPanel from "./HeroReplaceKeyPanel";

type Props = {
  workspaceId: string;
  provider: TenantProvider | null;
  credentials: Credential[];
};

const PANEL = { idle: "idle", changeModel: "changeModel", replaceKey: "replaceKey" } as const;
type Panel = (typeof PANEL)[keyof typeof PANEL];

const BILLING_PROVIDER_DIRECT = "Provider direct";
const BILLING_TENANT_BALANCE = "Tenant balance";
const MANAGED_PROVIDER = "agentsfleet managed";

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
    <DashboardPanel
      data-testid="active-model-hero"
      data-live={live}
      className="space-y-lg data-[live=true]:border-primary data-[live=true]:ring-1 data-[live=true]:ring-primary/20"
    >
      <div className="flex items-center gap-3">
        <StatusPill variant={live ? "pulse" : "neutral"} dot>
          {live ? "LIVE" : "DEFAULT"}
        </StatusPill>
        <span className={cn(EYEBROW_CLASS, "text-text-subtle")}>Active model</span>
      </div>

      {/* Heading in the sans display scale like every other page-level object;
          mono stays reserved for data (the credential ref below, IDs, code). */}
      <div>
        <div className="break-all text-display-md font-semibold leading-display-md tracking-display-md text-foreground">
          {live ? provider.model : "Platform default model"}
        </div>
        <div className="mt-1 text-body-sm leading-body-sm text-muted-foreground">
          {live ? (
            <>
              via <span className="font-mono">{credRef ?? provider.provider}</span>
            </>
          ) : (
            "Managed by agentsfleet · no key needed"
          )}
        </div>
      </div>

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
          <Button asChild size="sm">
            <a href="#other-providers">Bring your own key</a>
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

      {error ? (
        <Alert variant="destructive" className="text-xs">
          {error}
        </Alert>
      ) : null}
    </DashboardPanel>
  );
}
