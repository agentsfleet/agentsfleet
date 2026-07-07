"use client";

import { Button, DashboardRow, MetaGrid } from "@agentsfleet/design-system";
import { CpuIcon, LinkIcon, LockIcon, PlusIcon, ServerIcon } from "lucide-react";
import type { CustomEndpointSecret, ProviderKeySecret } from "@/lib/api/secrets";
import { providerLabel } from "@/lib/api/model_caps";
import ProviderKeyForm from "./ProviderKeyForm";
import CustomEndpointForm from "./CustomEndpointForm";
import {
  ADD_KEY_AND_MODEL_LABEL,
  ANTHROPIC_PROVIDER,
  CUSTOM_LABEL,
  DEFAULT_LABEL,
  LiveBadge,
  PANEL,
  PLATFORM_UNAVAILABLE_NOTE,
  type RowControls,
  addButton,
  deleteButton,
  editButtons,
  editPanel,
  formatContext,
  switchButton,
} from "./ProviderRowHelpers";

// ── Default row — always read-only; edited only via the admin Model Library ──
export function DefaultRow({
  live,
  platformDefaultAvailable,
  pending,
  onSwitchPlatform,
}: {
  live: boolean;
  platformDefaultAvailable: boolean;
  pending: boolean;
  onSwitchPlatform: () => void;
}) {
  return (
    <DashboardRow
      data-testid="row-default"
      icon={<ServerIcon size={15} />}
      title={
        <span className="inline-flex items-center gap-2">
          <span>{DEFAULT_LABEL}</span>
          <LockIcon size={12} className="text-muted-foreground" aria-label="Managed by a platform admin" />
        </span>
      }
      action={
        !live
          ? <LiveBadge />
          : platformDefaultAvailable
            ? switchButton(pending, onSwitchPlatform)
            : switchButton(pending, onSwitchPlatform, true, PLATFORM_UNAVAILABLE_NOTE)
      }
      meta={
        live && !platformDefaultAvailable ? (
          <p className="text-xs text-muted-foreground">{PLATFORM_UNAVAILABLE_NOTE}</p>
        ) : null
      }
    />
  );
}

// ── Anthropic row — first-class, always the same slot ──────────────────
export function AnthropicRow({
  anthropicSecret,
  isAnthropicActive,
  contextCapTokens,
  model,
  controls,
}: {
  anthropicSecret: ProviderKeySecret | null;
  isAnthropicActive: boolean;
  contextCapTokens: number | undefined;
  model: string;
  controls: RowControls;
}) {
  return (
    <>
      <DashboardRow
        data-testid="row-anthropic"
        icon={<CpuIcon size={15} />}
        title={providerLabel(ANTHROPIC_PROVIDER)}
        description={anthropicSecret ? (anthropicSecret.model ?? "model not set") : "Not configured"}
        action={
          isAnthropicActive
            ? <LiveBadge />
            : anthropicSecret
              ? (
                  <div className="flex items-center gap-1">
                    {switchButton(controls.pending, () => controls.onSwitch(anthropicSecret.name, anthropicSecret.model))}
                    {deleteButton(controls.pending, controls.onDelete, anthropicSecret.name, false)}
                  </div>
                )
              : addButton(
                  controls.pending,
                  controls.openRow === "anthropic" && controls.openPanel === PANEL.addKey,
                  ADD_KEY_AND_MODEL_LABEL,
                  () => controls.toggle("anthropic", PANEL.addKey),
                )
        }
        meta={
          isAnthropicActive && anthropicSecret ? (
            <div className="space-y-md">
              <MetaGrid
                bordered
                items={[
                  { label: "Provider", value: providerLabel(anthropicSecret.provider) },
                  { label: "Context", value: formatContext(contextCapTokens) },
                  { label: "Billing", value: "Provider direct" },
                ]}
              />
              {editButtons("anthropic", anthropicSecret, controls)}
            </div>
          ) : null
        }
      />
      {controls.openRow === "anthropic" && controls.openPanel === PANEL.addKey ? (
        <div className="border-t border-border bg-surface-deep p-lg">
          <ProviderKeyForm
            workspaceId={controls.workspaceId}
            provider={ANTHROPIC_PROVIDER}
            activate
            onDone={controls.close}
            onCancel={controls.close}
          />
        </div>
      ) : null}
      {isAnthropicActive && anthropicSecret ? editPanel("anthropic", anthropicSecret, model, controls) : null}
    </>
  );
}

// ── Other provider row — single slot, first-class-labeled by whoever's in it ──
export function OtherProviderRow({
  otherDisplay,
  activeOther,
  otherRest,
  contextCapTokens,
  model,
  controls,
}: {
  otherDisplay: ProviderKeySecret | null;
  activeOther: ProviderKeySecret | null;
  otherRest: ProviderKeySecret[];
  contextCapTokens: number | undefined;
  model: string;
  controls: RowControls;
}) {
  const otherTitle = otherDisplay ? `Other provider — ${providerLabel(otherDisplay.provider)}` : "Other provider";
  return (
    <>
      <DashboardRow
        data-testid="row-other"
        icon={<CpuIcon size={15} />}
        title={otherTitle}
        description={
          otherDisplay
            ? (otherDisplay.model ?? "model not set")
            : "Paste a key — we'll detect common providers, or pick one"
        }
        action={
          activeOther
            ? <LiveBadge />
            : otherDisplay
              ? (
                  <div className="flex items-center gap-1">
                    {switchButton(controls.pending, () => controls.onSwitch(otherDisplay.name, otherDisplay.model))}
                    {deleteButton(controls.pending, controls.onDelete, otherDisplay.name, false)}
                  </div>
                )
              : addButton(
                  controls.pending,
                  controls.openRow === "other" && controls.openPanel === PANEL.addKey,
                  ADD_KEY_AND_MODEL_LABEL,
                  () => controls.toggle("other", PANEL.addKey),
                )
        }
        meta={
          <div className="space-y-md">
            {activeOther ? (
              <>
                <MetaGrid
                  bordered
                  items={[
                    { label: "Provider", value: providerLabel(activeOther.provider) },
                    { label: "Context", value: formatContext(contextCapTokens) },
                    { label: "Billing", value: "Provider direct" },
                  ]}
                />
                {editButtons("other", activeOther, controls)}
              </>
            ) : null}
            {otherDisplay ? (
              <Button
                type="button"
                variant="ghost"
                size="sm"
                disabled={controls.pending}
                aria-expanded={controls.openRow === "other" && controls.openPanel === PANEL.addKey}
                onClick={() => controls.toggle("other", PANEL.addKey)}
                className="gap-1.5"
              >
                <PlusIcon size={14} />
                Add another
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
                      {switchButton(controls.pending, () => controls.onSwitch(k.name, k.model))}
                      {deleteButton(controls.pending, controls.onDelete, k.name, false)}
                    </div>
                  </div>
                ))}
              </div>
            ) : null}
          </div>
        }
      />
      {controls.openRow === "other" && controls.openPanel === PANEL.addKey ? (
        <div className="border-t border-border bg-surface-deep p-lg">
          <ProviderKeyForm workspaceId={controls.workspaceId} activate onDone={controls.close} onCancel={controls.close} />
        </div>
      ) : null}
      {activeOther ? editPanel("other", activeOther, model, controls) : null}
    </>
  );
}

// ── Custom — OpenAI-compatible row — unchanged, one endpoint slot ───────
export function CustomRow({
  activeCustomEndpoint,
  firstCustomEndpoint,
  controls,
}: {
  activeCustomEndpoint: CustomEndpointSecret | null;
  firstCustomEndpoint: CustomEndpointSecret | null;
  controls: RowControls;
}) {
  return (
    <>
      <DashboardRow
        data-testid="row-custom"
        icon={<LinkIcon size={15} />}
        title={CUSTOM_LABEL}
        description={
          activeCustomEndpoint
            ? `${activeCustomEndpoint.name} · ${activeCustomEndpoint.model ?? activeCustomEndpoint.base_url ?? "model not set"}`
            : "OpenAI-compatible gateway, OpenRouter, or self-hosted"
        }
        action={
          activeCustomEndpoint
            ? <LiveBadge />
            : firstCustomEndpoint
              ? switchButton(controls.pending, () => controls.onSwitch(firstCustomEndpoint.name, firstCustomEndpoint.model))
              : addButton(
                  controls.pending,
                  controls.openRow === "custom" && controls.openPanel === PANEL.addKey,
                  ADD_KEY_AND_MODEL_LABEL,
                  () => controls.toggle("custom", PANEL.addKey),
                )
        }
      />
      {controls.openRow === "custom" ? (
        <div className="border-t border-border bg-surface-deep p-lg">
          <CustomEndpointForm workspaceId={controls.workspaceId} activate onDone={controls.close} onCancel={controls.close} />
        </div>
      ) : null}
    </>
  );
}
