"use client";

import { useState } from "react";
import {
  Alert,
  Button,
  DashboardRow,
  DashboardRowGroup,
  SectionLabel,
  Spinner,
} from "@agentsfleet/design-system";
import { CpuIcon, LinkIcon, ServerIcon } from "lucide-react";
import { resetProviderAction, setProviderSelfManagedAction } from "../actions";
import {
  customEndpointsOf,
  providerKeysOf,
  type Secret,
} from "@/lib/api/secrets";
import { providerLabel, uniqueProviders } from "@/lib/api/model_caps";
import { OPENAI_COMPATIBLE_PROVIDER, PROVIDER_MODE, type TenantProvider } from "@/lib/types";
import { captureModelActivated, captureProviderReset } from "../lib/track";
import { useProviderAction } from "../lib/use-provider-action";
import { useModelCatalogue } from "./ModelCatalogueProvider";
import ProviderKeyForm from "./ProviderKeyForm";
import CustomEndpointForm from "./CustomEndpointForm";
import ActiveModelHero, { ADD_KEY_AND_MODEL_LABEL } from "./ActiveModelHero";

type Props = {
  workspaceId: string;
  provider: TenantProvider | null;
  credentials: Secret[];
};

const ADD_ENDPOINT_ROW = "__add_endpoint__";
const ADD_GENERIC_ROW = "__add_generic__";
const PLATFORM_LABEL = "Platform defaults";
const CUSTOM_LABEL = "Custom — OpenAI-compatible";
const SWITCH_ACTION = "switch providers";
const SWITCH_PLATFORM_ACTION = "switch to platform defaults";

export default function ProviderSwitchList({ workspaceId, provider, credentials }: Props) {
  const { models } = useModelCatalogue();
  const [open, setOpen] = useState<string | null>(null);
  const { pending, error, run } = useProviderAction();
  const closeOpen = () => setOpen(null);

  const live = provider?.mode === PROVIDER_MODE.self_managed;
  const activeRef = live ? provider.secret_ref : null;
  const providerKeys = providerKeysOf(credentials);
  const customEndpoints = customEndpointsOf(credentials);

  // Named providers the switch list offers: the catalogue's providers unioned
  // with any the workspace already stored a key for, minus the openai-compatible
  // id (it gets the dedicated custom-endpoint rows below).
  const namedProviders = Array.from(
    new Set([...uniqueProviders(models), ...providerKeys.map((k) => k.provider)]),
  ).filter((p) => p !== OPENAI_COMPATIBLE_PROVIDER);

  function onSwitch(secretRef: string, model?: string) {
    void run(
      SWITCH_ACTION,
      async () => {
        const res = await setProviderSelfManagedAction({ secret_ref: secretRef, model });
        if (!res.ok) return { message: res.error, errorCode: res.errorCode };
        captureModelActivated(res.data);
        return null;
      },
      closeOpen,
    );
  }

  function onSwitchPlatform(fromProvider: string) {
    void run(
      SWITCH_PLATFORM_ACTION,
      async () => {
        const res = await resetProviderAction();
        if (!res.ok) return { message: res.error, errorCode: res.errorCode };
        captureProviderReset(fromProvider);
        return null;
      },
      closeOpen,
    );
  }

  function toggle(id: string) {
    setOpen((cur) => (cur === id ? null : id));
  }

  function switchButton(onClick: () => void) {
    return (
      <Button type="button" size="sm" disabled={pending} onClick={onClick}>
        Switch
      </Button>
    );
  }

  function addButton(id: string, label: string) {
    return (
      <Button
        type="button"
        size="sm"
        variant="outline"
        disabled={pending}
        aria-expanded={open === id}
        onClick={() => toggle(id)}
      >
        {label}
      </Button>
    );
  }

  return (
    <div aria-label="Providers">
      <SectionLabel>Providers</SectionLabel>
      {pending ? <Spinner size="sm" srLabel="Switching" /> : null}
      <DashboardRowGroup data-testid="provider-switch-list">
        <ActiveModelHero workspaceId={workspaceId} provider={provider} credentials={credentials} />

        {/* Platform defaults — shown only while a self-managed model is live. */}
        {live ? (
          <DashboardRow
            icon={<ServerIcon size={15} />}
            title={PLATFORM_LABEL}
            description="Built-in provider · no key"
            action={switchButton(() => onSwitchPlatform(provider.provider))}
          />
        ) : null}

        {/* One row per named provider: Switch when keyed, Add key & model when not. */}
        {namedProviders.map((p) => {
          const storedKey = providerKeys.find((k) => k.provider === p);
          if (storedKey && storedKey.name === activeRef) return null; // the live hero
          return (
            <div key={p}>
              <DashboardRow
                icon={<CpuIcon size={15} />}
                title={providerLabel(p)}
                description={storedKey ? `Key saved · ${storedKey.model ?? "model not set"}` : "Not configured"}
                action={
                  storedKey
                    ? switchButton(() => onSwitch(storedKey.name, storedKey.model))
                    : addButton(p, ADD_KEY_AND_MODEL_LABEL)
                }
              />
              {open === p ? (
                <div className="border-t border-border bg-surface-deep p-lg">
                  <ProviderKeyForm
                    workspaceId={workspaceId}
                    provider={p}
                    activate
                    onDone={() => setOpen(null)}
                    onCancel={() => setOpen(null)}
                  />
                </div>
              ) : null}
            </div>
          );
        })}

        {/* Stored custom endpoints (non-active) → one-click Switch. */}
        {customEndpoints
          .filter((ce) => ce.name !== activeRef)
          .map((ce) => (
            <DashboardRow
              key={ce.name}
              icon={<LinkIcon size={15} />}
              title={CUSTOM_LABEL}
              description={`${ce.name} · ${ce.model ?? ce.base_url ?? "model not set"}`}
              action={switchButton(() => onSwitch(ce.name, ce.model))}
            />
          ))}

        {/* Add a new custom endpoint. */}
        <div>
          <DashboardRow
            icon={<LinkIcon size={15} />}
            title={CUSTOM_LABEL}
            description="OpenAI-compatible gateway, OpenRouter, or self-hosted"
            action={addButton(ADD_ENDPOINT_ROW, "Add endpoint")}
          />
          {open === ADD_ENDPOINT_ROW ? (
            <div className="border-t border-border bg-surface-deep p-lg">
              <CustomEndpointForm
                workspaceId={workspaceId}
                activate
                onDone={() => setOpen(null)}
                onCancel={() => setOpen(null)}
              />
            </div>
          ) : null}
        </div>

        {/* Generic add for a provider the catalogue doesn't enumerate (paste-detect). */}
        <div>
          <DashboardRow
            icon={<CpuIcon size={15} />}
            title="Other provider"
            description="Paste a key — we'll detect common providers, or pick one"
            action={addButton(ADD_GENERIC_ROW, ADD_KEY_AND_MODEL_LABEL)}
          />
          {open === ADD_GENERIC_ROW ? (
            <div className="border-t border-border bg-surface-deep p-lg">
              <ProviderKeyForm
                workspaceId={workspaceId}
                activate
                onDone={() => setOpen(null)}
                onCancel={() => setOpen(null)}
              />
            </div>
          ) : null}
        </div>
      </DashboardRowGroup>

      {error ? (
        <Alert variant="destructive" className="mt-md text-xs">
          {error}
        </Alert>
      ) : null}
    </div>
  );
}
