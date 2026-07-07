"use client";

import { useState } from "react";
import {
  Alert,
  Button,
  DashboardRow,
  DashboardRowGroup,
  MetaGrid,
  SectionLabel,
  Spinner,
} from "@agentsfleet/design-system";
import { CpuIcon, LinkIcon, LockIcon, ServerIcon, Trash2Icon } from "lucide-react";
import { resetProviderAction, setProviderSelfManagedAction } from "../actions";
import { deleteSecretAction } from "@/app/(dashboard)/w/[workspaceId]/secrets/actions";
import {
  customEndpointsOf,
  providerKeysOf,
  type ProviderKeySecret,
  type Secret,
} from "@/lib/api/secrets";
import { providerLabel } from "@/lib/api/model_caps";
import { PROVIDER_MODE, type TenantProvider } from "@/lib/types";
import { captureModelActivated, captureProviderReset } from "../lib/track";
import { useProviderAction } from "../lib/use-provider-action";
import HeroChangeModelPanel from "./HeroChangeModelPanel";
import HeroReplaceKeyPanel from "./HeroReplaceKeyPanel";
import ProviderKeyForm from "./ProviderKeyForm";
import CustomEndpointForm from "./CustomEndpointForm";

type Props = {
  workspaceId: string;
  provider: TenantProvider | null;
  secrets: Secret[];
};

const ANTHROPIC_PROVIDER = "anthropic";

export const ADD_KEY_AND_MODEL_LABEL = "Add key & model";
const DEFAULT_LABEL = "Default";
const CUSTOM_LABEL = "Custom — OpenAI-compatible";
const DEFAULT_DESCRIPTION = "Add your own key to run on a different provider.";
const PLATFORM_UNAVAILABLE_NOTE = "No platform default is configured on this deployment yet.";
const SWITCH_ACTION = "switch providers";
const SWITCH_PLATFORM_ACTION = "switch to platform defaults";
const DELETE_ACTION = "delete the stored key";

const PANEL = { addKey: "addKey", changeModel: "changeModel", replaceKey: "replaceKey" } as const;
type PanelKind = (typeof PANEL)[keyof typeof PANEL];
type OpenRow = "anthropic" | "other" | "custom" | null;

// Threshold + divisor for the "k" context abbreviation (200000 → "200k").
const TOKENS_PER_K = 1000;

function formatContext(tokens: number | undefined): string {
  if (!tokens || tokens <= 0) return "default";
  return tokens >= TOKENS_PER_K ? `${Math.round(tokens / TOKENS_PER_K)}k` : String(tokens);
}

function LiveBadge() {
  return (
    <span className="inline-flex items-center gap-1 text-xs font-semibold text-pulse">
      <span className="h-1.5 w-1.5 rounded-full bg-pulse" aria-hidden="true" />
      Live
    </span>
  );
}

export default function ProviderSwitchList({ workspaceId, provider, secrets }: Props) {
  const [openRow, setOpenRow] = useState<OpenRow>(null);
  const [openPanel, setOpenPanel] = useState<PanelKind>(PANEL.addKey);
  const { pending, error, run } = useProviderAction();

  const live = provider?.mode === PROVIDER_MODE.self_managed;
  const activeRef = live ? provider.secret_ref : null;
  const platformDefaultAvailable = provider?.platform_default_available ?? false;

  const providerKeys = providerKeysOf(secrets);
  const customEndpoints = customEndpointsOf(secrets);

  const anthropicSecret = providerKeys.find((k) => k.provider === ANTHROPIC_PROVIDER) ?? null;
  const isAnthropicActive = Boolean(live && anthropicSecret?.name === activeRef);

  const otherSecrets = providerKeys.filter((k) => k.provider !== ANTHROPIC_PROVIDER);
  const activeOther = live ? (otherSecrets.find((k) => k.name === activeRef) ?? null) : null;
  const otherDisplay = activeOther ?? otherSecrets[0] ?? null;
  const otherRest = otherDisplay ? otherSecrets.filter((k) => k.name !== otherDisplay.name) : [];

  const activeCustomEndpoint = live ? (customEndpoints.find((c) => c.name === activeRef) ?? null) : null;
  const firstCustomEndpoint = customEndpoints[0] ?? null;

  function close() {
    setOpenRow(null);
  }

  function toggle(row: OpenRow, panel: PanelKind) {
    setOpenRow((cur) => (cur === row && openPanel === panel ? null : row));
    setOpenPanel(panel);
  }

  function onSwitch(secretRef: string, model?: string) {
    void run(
      SWITCH_ACTION,
      async () => {
        const res = await setProviderSelfManagedAction({ secret_ref: secretRef, model });
        if (!res.ok) return { message: res.error, errorCode: res.errorCode };
        captureModelActivated(res.data);
        return null;
      },
      close,
    );
  }

  function onSwitchPlatform() {
    if (!platformDefaultAvailable) return;
    const fromProvider = live ? provider.provider : "";
    void run(
      SWITCH_PLATFORM_ACTION,
      async () => {
        const res = await resetProviderAction();
        if (!res.ok) return { message: res.error, errorCode: res.errorCode };
        captureProviderReset(fromProvider);
        return null;
      },
      close,
    );
  }

  function onDelete(name: string) {
    void run(
      DELETE_ACTION,
      async () => {
        const res = await deleteSecretAction(workspaceId, name);
        if (!res.ok) return { message: res.error, errorCode: res.errorCode };
        return null;
      },
      close,
    );
  }

  function switchButton(onClick: () => void, disabled?: boolean, title?: string) {
    return (
      <Button type="button" size="sm" disabled={pending || disabled} onClick={onClick} title={title}>
        Switch
      </Button>
    );
  }

  function deleteButton(name: string, disabled: boolean) {
    return (
      <Button
        type="button"
        variant="ghost"
        size="sm"
        disabled={pending || disabled}
        onClick={() => onDelete(name)}
        aria-label={disabled ? `Cannot delete ${name} while it is active` : `Delete ${name}`}
      >
        <Trash2Icon size={14} />
      </Button>
    );
  }

  function addButton(row: OpenRow, label: string) {
    return (
      <Button
        type="button"
        size="sm"
        variant="outline"
        disabled={pending}
        aria-expanded={openRow === row && openPanel === PANEL.addKey}
        onClick={() => toggle(row, PANEL.addKey)}
      >
        {label}
      </Button>
    );
  }

  function editButtons(row: "anthropic" | "other", secret: ProviderKeySecret) {
    return (
      <div className="flex flex-wrap items-center gap-md">
        <Button
          type="button"
          variant="outline"
          size="sm"
          aria-expanded={openRow === row && openPanel === PANEL.changeModel}
          onClick={() => toggle(row, PANEL.changeModel)}
        >
          Change model
        </Button>
        <Button
          type="button"
          variant="outline"
          size="sm"
          aria-expanded={openRow === row && openPanel === PANEL.replaceKey}
          onClick={() => toggle(row, PANEL.replaceKey)}
        >
          Replace key
        </Button>
        {deleteButton(secret.name, true)}
      </div>
    );
  }

  function editPanel(row: "anthropic" | "other", secret: ProviderKeySecret, model: string) {
    if (openRow !== row) return null;
    if (openPanel === PANEL.changeModel) {
      return <HeroChangeModelPanel provider={secret.provider} secretRef={secret.name} onClose={close} />;
    }
    if (openPanel === PANEL.replaceKey) {
      return (
        <HeroReplaceKeyPanel
          workspaceId={workspaceId}
          secretRef={secret.name}
          provider={secret.provider}
          currentModel={model}
          onClose={close}
        />
      );
    }
    return null;
  }

  // ── Default row — always read-only; edited only via the admin Model Library ──
  const defaultRow = (
    <DashboardRow
      data-testid="row-default"
      icon={<ServerIcon size={15} />}
      title={
        <span className="inline-flex items-center gap-2">
          <span>{DEFAULT_LABEL}</span>
          <LockIcon size={12} className="text-muted-foreground" aria-label="Managed by a platform admin" />
          {!live ? <LiveBadge /> : null}
        </span>
      }
      description={DEFAULT_DESCRIPTION}
      action={
        !live
          ? null
          : platformDefaultAvailable
            ? switchButton(onSwitchPlatform)
            : switchButton(onSwitchPlatform, true, PLATFORM_UNAVAILABLE_NOTE)
      }
      meta={
        live && !platformDefaultAvailable ? (
          <p className="text-xs text-muted-foreground">{PLATFORM_UNAVAILABLE_NOTE}</p>
        ) : null
      }
    />
  );

  // ── Anthropic row — first-class, always the same slot ──────────────────
  const anthropicRow = (
    <div>
      <DashboardRow
        data-testid="row-anthropic"
        icon={<CpuIcon size={15} />}
        title={
          <span className="inline-flex items-center gap-2">
            <span>{providerLabel(ANTHROPIC_PROVIDER)}</span>
            {isAnthropicActive ? <LiveBadge /> : null}
          </span>
        }
        description={anthropicSecret ? `Key saved · ${anthropicSecret.model ?? "model not set"}` : "Not configured"}
        action={
          isAnthropicActive
            ? null
            : anthropicSecret
              ? (
                  <div className="flex items-center gap-1">
                    {switchButton(() => onSwitch(anthropicSecret.name, anthropicSecret.model))}
                    {deleteButton(anthropicSecret.name, false)}
                  </div>
                )
              : addButton("anthropic", ADD_KEY_AND_MODEL_LABEL)
        }
        meta={
          isAnthropicActive && anthropicSecret ? (
            <div className="space-y-md">
              <MetaGrid
                bordered
                items={[
                  { label: "Provider", value: providerLabel(anthropicSecret.provider) },
                  { label: "Context", value: formatContext(provider?.context_cap_tokens) },
                  { label: "Billing", value: "Provider direct" },
                ]}
              />
              {editButtons("anthropic", anthropicSecret)}
            </div>
          ) : null
        }
      />
      {openRow === "anthropic" && openPanel === PANEL.addKey ? (
        <div className="border-t border-border bg-surface-deep p-lg">
          <ProviderKeyForm workspaceId={workspaceId} provider={ANTHROPIC_PROVIDER} activate onDone={close} onCancel={close} />
        </div>
      ) : null}
      {isAnthropicActive && anthropicSecret ? editPanel("anthropic", anthropicSecret, provider?.model ?? "") : null}
    </div>
  );

  // ── Other provider row — single slot, first-class-labeled by whoever's in it ──
  const otherTitle = otherDisplay ? `Other provider — ${providerLabel(otherDisplay.provider)}` : "Other provider";
  const otherRow = (
    <div>
      <DashboardRow
        data-testid="row-other"
        icon={<CpuIcon size={15} />}
        title={
          <span className="inline-flex items-center gap-2">
            <span>{otherTitle}</span>
            {activeOther ? <LiveBadge /> : null}
          </span>
        }
        description={
          otherDisplay
            ? `Key saved · ${otherDisplay.model ?? "model not set"}`
            : "Paste a key — we'll detect common providers, or pick one"
        }
        action={
          activeOther
            ? null
            : otherDisplay
              ? (
                  <div className="flex items-center gap-1">
                    {switchButton(() => onSwitch(otherDisplay.name, otherDisplay.model))}
                    {deleteButton(otherDisplay.name, false)}
                  </div>
                )
              : addButton("other", ADD_KEY_AND_MODEL_LABEL)
        }
        meta={
          <div className="space-y-md">
            {activeOther ? (
              <>
                <MetaGrid
                  bordered
                  items={[
                    { label: "Provider", value: providerLabel(activeOther.provider) },
                    { label: "Context", value: formatContext(provider?.context_cap_tokens) },
                    { label: "Billing", value: "Provider direct" },
                  ]}
                />
                {editButtons("other", activeOther)}
              </>
            ) : null}
            {otherDisplay ? (
              <Button
                type="button"
                variant="ghost"
                size="sm"
                disabled={pending}
                aria-expanded={openRow === "other" && openPanel === PANEL.addKey}
                onClick={() => toggle("other", PANEL.addKey)}
              >
                + Add another
              </Button>
            ) : null}
            {/* Dimension 3.2 — every other stored non-Anthropic key stays reachable,
                none silently hidden when a new one is added. */}
            {otherRest.length > 0 ? (
              <div className="space-y-1">
                {otherRest.map((k) => (
                  <div key={k.name} className="flex items-center justify-between gap-2 text-sm">
                    <span>
                      {providerLabel(k.provider)} ·{" "}
                      <span className="font-mono text-xs">{k.model ?? "model not set"}</span>
                    </span>
                    <div className="flex items-center gap-1">
                      {switchButton(() => onSwitch(k.name, k.model))}
                      {deleteButton(k.name, false)}
                    </div>
                  </div>
                ))}
              </div>
            ) : null}
          </div>
        }
      />
      {openRow === "other" && openPanel === PANEL.addKey ? (
        <div className="border-t border-border bg-surface-deep p-lg">
          <ProviderKeyForm workspaceId={workspaceId} activate onDone={close} onCancel={close} />
        </div>
      ) : null}
      {activeOther ? editPanel("other", activeOther, provider?.model ?? "") : null}
    </div>
  );

  // ── Custom — OpenAI-compatible row — unchanged, one endpoint slot ───────
  const customRow = (
    <div>
      <DashboardRow
        data-testid="row-custom"
        icon={<LinkIcon size={15} />}
        title={
          <span className="inline-flex items-center gap-2">
            <span>{CUSTOM_LABEL}</span>
            {activeCustomEndpoint ? <LiveBadge /> : null}
          </span>
        }
        description={
          activeCustomEndpoint
            ? `${activeCustomEndpoint.name} · ${activeCustomEndpoint.model ?? activeCustomEndpoint.base_url ?? "model not set"}`
            : "OpenAI-compatible gateway, OpenRouter, or self-hosted"
        }
        action={
          activeCustomEndpoint
            ? null
            : firstCustomEndpoint
              ? switchButton(() => onSwitch(firstCustomEndpoint.name, firstCustomEndpoint.model))
              : addButton("custom", "Add endpoint")
        }
      />
      {openRow === "custom" ? (
        <div className="border-t border-border bg-surface-deep p-lg">
          <CustomEndpointForm workspaceId={workspaceId} activate onDone={close} onCancel={close} />
        </div>
      ) : null}
    </div>
  );

  return (
    <div aria-label="Providers">
      <SectionLabel>Providers</SectionLabel>
      {pending ? <Spinner size="sm" srLabel="Switching" /> : null}
      <DashboardRowGroup data-testid="provider-switch-list">
        {defaultRow}
        {anthropicRow}
        {otherRow}
        {customRow}
      </DashboardRowGroup>

      {error ? (
        <Alert variant="destructive" className="mt-md text-xs">
          {error}
        </Alert>
      ) : null}
    </div>
  );
}
