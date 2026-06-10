"use client";

import { useActionState, useState } from "react";
import { useRouter } from "next/navigation";
import { ActionForm, Alert, Badge, Button, RadioGroup, Spinner } from "@usezombie/design-system";
import { resetProviderAction, setProviderSelfManagedAction } from "../actions";
import type { ActionResult } from "@/lib/actions/with-token";
import type { CredentialSummary } from "@/lib/api/credentials";
import type { ModelCap } from "@/lib/api/model_caps";
import { PROVIDER_MODE, type ProviderMode, type TenantProvider } from "@/lib/types";
import { EVENTS } from "@/lib/analytics/events";
import { captureProductEvent } from "@/lib/analytics/posthog";
import ModeRadio from "./ModeRadio";
import Step1Credential from "./Step1Credential";
import Step2Model from "./Step2Model";

type Props = {
  workspaceId: string;
  currentMode: ProviderMode;
  currentCredentialRef: string | null;
  currentModel: string;
  credentials: CredentialSummary[];
  catalogue: ModelCap[];
};

type ActionState = { ok: string | null; error: string | null };

type ModeStrategy = {
  submitLabel: string;
  successMsg: string;
  run: (form: { credentialRef: string; modelOverride: string }) => Promise<ActionResult<TenantProvider>>;
};

const MODE_STRATEGIES: Record<ProviderMode, ModeStrategy> = {
  platform: {
    submitLabel: "Use platform defaults",
    successMsg: "Using platform defaults.",
    run: () => resetProviderAction(),
  },
  self_managed: {
    submitLabel: "Save model setup",
    successMsg: "Saved. Run a test event to verify the key.",
    run: ({ credentialRef, modelOverride }) =>
      setProviderSelfManagedAction({
        credential_ref: credentialRef,
        model: modelOverride.trim() || undefined,
      }),
  },
};

const INITIAL_ACTION_STATE: ActionState = { ok: null, error: null };

function PlatformModePanel({ isPending }: { isPending: boolean }) {
  return (
    <div className="space-y-4">
      <div className="grid gap-3 sm:grid-cols-3">
        <div>
          <div className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
            Credential
          </div>
          <div className="mt-1 font-medium">Not required</div>
        </div>
        <div>
          <div className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
            Account
          </div>
          <div className="mt-1 font-medium">usezombie managed</div>
        </div>
        <div>
          <div className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
            Billing
          </div>
          <div className="mt-1 font-medium">Tenant balance</div>
        </div>
      </div>
      <Button type="submit" disabled={isPending}>
        {isPending ? <Spinner size="sm" srLabel="Saving" /> : null}
        {MODE_STRATEGIES.platform.submitLabel}
      </Button>
    </div>
  );
}

type SelfManagedModePanelProps = {
  workspaceId: string;
  credentials: CredentialSummary[];
  catalogue: ModelCap[];
  credentialRef: string;
  modelOverride: string;
  isPending: boolean;
  onCredentialRefChange: (ref: string) => void;
  onModelChange: (value: string) => void;
};

function SelfManagedModePanel({
  workspaceId,
  credentials,
  catalogue,
  credentialRef,
  modelOverride,
  isPending,
  onCredentialRefChange,
  onModelChange,
}: SelfManagedModePanelProps) {
  return (
    <div className="space-y-4">
      <div className="grid gap-4 lg:grid-cols-2">
        <Step1Credential
          workspaceId={workspaceId}
          credentials={credentials}
          catalogue={catalogue}
          credentialRef={credentialRef}
          onCredentialRefChange={onCredentialRefChange}
        />
        <Step2Model catalogue={catalogue} model={modelOverride} onModelChange={onModelChange} />
      </div>
      <Button type="submit" disabled={isPending || credentialRef === ""}>
        {isPending ? <Spinner size="sm" srLabel="Saving" /> : null}
        {MODE_STRATEGIES.self_managed.submitLabel}
      </Button>
    </div>
  );
}

function PlatformModeCard({
  mode,
  isPending,
}: {
  mode: ProviderMode;
  isPending: boolean;
}) {
  return (
    <ModeRadio
      value={PROVIDER_MODE.platform}
      checked={mode === PROVIDER_MODE.platform}
      label="Platform defaults"
      meta="No key"
      description="Use the built-in provider and pay from your tenant balance per event."
    >
      <PlatformModePanel isPending={isPending} />
    </ModeRadio>
  );
}

function SelfManagedModeCard({
  mode,
  workspaceId,
  credentials,
  catalogue,
  credentialRef,
  modelOverride,
  isPending,
  onCredentialRefChange,
  onModelChange,
}: SelfManagedModePanelProps & { mode: ProviderMode }) {
  return (
    <ModeRadio
      value={PROVIDER_MODE.self_managed}
      checked={mode === PROVIDER_MODE.self_managed}
      label="Use my provider key"
      meta="Bring a key"
      description="Store a provider key, then choose which credential and model agents should use."
    >
      <SelfManagedModePanel
        workspaceId={workspaceId}
        credentials={credentials}
        catalogue={catalogue}
        credentialRef={credentialRef}
        modelOverride={modelOverride}
        isPending={isPending}
        onCredentialRefChange={onCredentialRefChange}
        onModelChange={onModelChange}
      />
    </ModeRadio>
  );
}

function ProviderSelectorFeedback({ state }: { state: ActionState }) {
  return (
    <>
      {state.ok ? (
        <Badge
          variant="green"
          role="status" // oxlint-disable-line jsx-a11y/prefer-tag-over-role -- Badge is the design-system primitive; <output> drops text children in happy-dom@20.
          className="animate-in fade-in-0 slide-in-from-bottom-1 duration-200 normal-case tracking-normal"
        >
          {state.ok}
        </Badge>
      ) : null}

      {state.error ? (
        <Alert variant="destructive" className="text-xs">
          {state.error}
        </Alert>
      ) : null}
    </>
  );
}

function ProviderModeRadioGroup({
  mode,
  workspaceId,
  credentials,
  catalogue,
  credentialRef,
  modelOverride,
  isPending,
  onModeChange,
  onCredentialRefChange,
  onModelChange,
}: SelfManagedModePanelProps & {
  mode: ProviderMode;
  onModeChange: (value: ProviderMode) => void;
}) {
  return (
    <fieldset>
      <legend className="sr-only">Model billing mode</legend>
      <RadioGroup
        value={mode}
        onValueChange={(v) => onModeChange(v as ProviderMode)}
        aria-label="Provider mode"
        className="space-y-3"
      >
        <PlatformModeCard mode={mode} isPending={isPending} />
        <SelfManagedModeCard
          mode={mode}
          workspaceId={workspaceId}
          credentials={credentials}
          catalogue={catalogue}
          credentialRef={credentialRef}
          modelOverride={modelOverride}
          isPending={isPending}
          onCredentialRefChange={onCredentialRefChange}
          onModelChange={onModelChange}
        />
      </RadioGroup>
    </fieldset>
  );
}

export default function ProviderSelector({
  workspaceId,
  currentMode,
  currentCredentialRef,
  currentModel,
  credentials,
  catalogue,
}: Props) {
  const router = useRouter();

  // Form-controlled inputs are local state; the action below is the React 19
  // form-action handler submitted by ActionForm.
  const [mode, setMode] = useState<ProviderMode>(currentMode);
  const [credentialRef, setCredentialRef] = useState<string>(
    currentCredentialRef ?? credentials[0]?.name ?? "",
  );
  const [modelOverride, setModelOverride] = useState<string>(
    currentMode === PROVIDER_MODE.self_managed ? currentModel : "",
  );

  const strategy = MODE_STRATEGIES[mode];

  async function action(_prev: ActionState, _formData: FormData): Promise<ActionState> {
    const result = await strategy.run({ credentialRef, modelOverride });
    if (!result.ok) return { ok: null, error: result.error };
    // A BYOK save is the funnel event; the platform-defaults submit removes
    // the key setup and emits nothing.
    if (result.data.mode === PROVIDER_MODE.self_managed) {
      captureProductEvent(EVENTS.model_added, {
        provider: result.data.provider,
        mode: result.data.mode,
        model: result.data.model,
      });
    }
    router.refresh();
    return { ok: strategy.successMsg, error: null };
  }

  const [state, submitAction, isPending] = useActionState(action, INITIAL_ACTION_STATE);

  return (
    <ActionForm action={submitAction} className="text-sm">
      <ProviderModeRadioGroup
        mode={mode}
        workspaceId={workspaceId}
        credentials={credentials}
        catalogue={catalogue}
        credentialRef={credentialRef}
        modelOverride={modelOverride}
        isPending={isPending}
        onModeChange={setMode}
        onCredentialRefChange={setCredentialRef}
        onModelChange={setModelOverride}
      />
      <p className="text-xs text-muted-foreground">
        Changes apply to new events; events already in flight finish on their current configuration.
      </p>

      <ProviderSelectorFeedback state={state} />
    </ActionForm>
  );
}
