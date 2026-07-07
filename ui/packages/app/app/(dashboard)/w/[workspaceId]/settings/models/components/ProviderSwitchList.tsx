"use client";

import { useState } from "react";
import { Alert, DashboardRowGroup, SectionLabel, Spinner } from "@agentsfleet/design-system";
import { resetProviderAction, setProviderSelfManagedAction } from "../actions";
import { deleteSecretAction } from "@/app/(dashboard)/w/[workspaceId]/secrets/actions";
import { customEndpointsOf, providerKeysOf, type Secret } from "@/lib/api/secrets";
import { PROVIDER_MODE, type TenantProvider } from "@/lib/types";
import { captureModelActivated, captureProviderReset } from "../lib/track";
import { useProviderAction } from "../lib/use-provider-action";
import { ANTHROPIC_PROVIDER, PANEL, type OpenRow, type PanelKind, type RowControls } from "./ProviderRowHelpers";
import { AnthropicRow, CustomRow, DefaultRow, OtherProviderRow } from "./ProviderRows";

type Props = {
  workspaceId: string;
  provider: TenantProvider | null;
  secrets: Secret[];
};

const SWITCH_ACTION = "switch providers";
const SWITCH_PLATFORM_ACTION = "switch to platform defaults";
const DELETE_ACTION = "delete the stored key";

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

  const controls: RowControls = { workspaceId, pending, openRow, openPanel, toggle, close, onSwitch, onDelete };

  return (
    <div aria-label="Providers">
      <SectionLabel>Providers</SectionLabel>
      {pending ? <Spinner size="sm" srLabel="Switching" /> : null}
      <DashboardRowGroup data-testid="provider-switch-list">
        <DefaultRow
          live={live}
          platformDefaultAvailable={platformDefaultAvailable}
          pending={pending}
          onSwitchPlatform={onSwitchPlatform}
        />
        <AnthropicRow
          anthropicSecret={anthropicSecret}
          isAnthropicActive={isAnthropicActive}
          contextCapTokens={provider?.context_cap_tokens}
          model={provider?.model ?? ""}
          controls={controls}
        />
        <OtherProviderRow
          otherDisplay={otherDisplay}
          activeOther={activeOther}
          otherRest={otherRest}
          contextCapTokens={provider?.context_cap_tokens}
          model={provider?.model ?? ""}
          controls={controls}
        />
        <CustomRow
          activeCustomEndpoint={activeCustomEndpoint}
          firstCustomEndpoint={firstCustomEndpoint}
          controls={controls}
        />
      </DashboardRowGroup>

      {error ? (
        <Alert variant="destructive" className="mt-md text-xs">
          {error}
        </Alert>
      ) : null}
    </div>
  );
}
