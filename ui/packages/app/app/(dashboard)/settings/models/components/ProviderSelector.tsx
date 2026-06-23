"use client";

import { useActionState, useState } from "react";
import { useRouter } from "next/navigation";
import { ActionForm, Alert, Badge, Button, Spinner } from "@agentsfleet/design-system";
import { resetProviderAction, setProviderSelfManagedAction } from "../actions";
import type { CredentialSummary } from "@/lib/api/credentials";
import type { ModelCap } from "@/lib/api/model_caps";
import { PROVIDER_MODE, type ProviderMode } from "@/lib/types";
import { EVENTS } from "@/lib/analytics/events";
import { captureProductEvent } from "@/lib/analytics/posthog";
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

const INITIAL_ACTION_STATE: ActionState = { ok: null, error: null };
const SAVE_SUCCESS_MSG = "Saved. Run a test event to verify the key.";
const PLATFORM_SUCCESS_MSG = "Using platform defaults.";

// Each option card's static facts (mirrors the meta grid in the mockup) —
// data, not branches, so the two cards render through one renderer.
const CARD_META: Record<ProviderMode, { credential: string; account: string; billing: string }> = {
  platform: { credential: "Not required", account: "agentsfleet managed", billing: "Tenant balance" },
  self_managed: { credential: "Required", account: "Your provider", billing: "Provider direct" },
};

function CardMeta({ mode }: { mode: ProviderMode }) {
  const meta = CARD_META[mode];
  return (
    <dl className="grid grid-cols-3 gap-3 text-xs">
      {(
        [
          ["Credential", meta.credential],
          ["Account", meta.account],
          ["Billing", meta.billing],
        ] as const
      ).map(([label, value]) => (
        <div key={label}>
          <dt className="font-mono text-label uppercase tracking-label text-muted-foreground">
            {label}
          </dt>
          <dd className="mt-1 font-medium text-foreground">{value}</dd>
        </div>
      ))}
    </dl>
  );
}

function ActiveFoot() {
  return (
    <div
      className="flex items-center gap-2 text-xs text-success"
      data-testid="active-note"
    >
      <span className="inline-block h-2 w-2 rounded-full bg-success" aria-hidden="true" />
      Active — nothing to do
    </div>
  );
}

type OptionCardProps = {
  mode: ProviderMode;
  title: string;
  description: string;
  modelLine: string;
  isActive: boolean;
  action: React.ReactNode;
};

function OptionCard({ mode, title, description, modelLine, isActive, action }: OptionCardProps) {
  return (
    <div
      data-active={isActive}
      data-testid={`option-card-${mode}`}
      className="flex flex-col gap-4 rounded-md border border-border bg-card p-4 data-[active=true]:border-primary"
    >
      <div className="space-y-1">
        <div className="flex flex-wrap items-center gap-2">
          <span className="font-medium text-foreground">{title}</span>
          {isActive ? <Badge variant="cyan">Current</Badge> : null}
        </div>
        <p className="text-xs text-muted-foreground">{description}</p>
      </div>
      <div className="break-all font-mono text-xs text-muted-foreground">{modelLine}</div>
      <CardMeta mode={mode} />
      <div className="mt-auto pt-1">{isActive ? <ActiveFoot /> : action}</div>
    </div>
  );
}

function OwnKeyConfig({
  workspaceId,
  credentials,
  catalogue,
  credentialRef,
  modelOverride,
  isPending,
  onCredentialRefChange,
  onModelChange,
  onCancel,
}: {
  workspaceId: string;
  credentials: CredentialSummary[];
  catalogue: ModelCap[];
  credentialRef: string;
  modelOverride: string;
  isPending: boolean;
  onCredentialRefChange: (ref: string) => void;
  onModelChange: (value: string) => void;
  onCancel: () => void;
}) {
  return (
    <div className="space-y-4 rounded-md border border-border bg-card p-4">
      <div className="space-y-1">
        <h3 className="font-mono text-heading text-foreground">Own-key model setup</h3>
        <p className="text-xs text-muted-foreground">
          Pick the stored credential and name the model teammates should use.
        </p>
      </div>
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
      <div className="flex flex-wrap gap-2">
        <Button type="submit" disabled={isPending || credentialRef === ""}>
          {isPending ? <Spinner size="sm" srLabel="Saving" /> : null}
          Save model setup
        </Button>
        <Button type="button" variant="ghost" disabled={isPending} onClick={onCancel}>
          Cancel
        </Button>
      </div>
    </div>
  );
}

function ProviderSelectorFeedback({ state }: { state: ActionState }) {
  return (
    <>
      {state.ok ? (
        <Badge
          variant="green"
          role="status" // oxlint-disable-line jsx-a11y/prefer-tag-over-role -- Badge is the design-system primitive; <output> drops text children in happy-dom@20.
          className="normal-case tracking-normal"
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

export default function ProviderSelector({
  workspaceId,
  currentMode,
  currentCredentialRef,
  currentModel,
  credentials,
  catalogue,
}: Props) {
  const router = useRouter();

  // The own-key config form is revealed by "Switch to own key"; the cards are
  // the default view. Form inputs are local state submitted by ActionForm.
  const [configuring, setConfiguring] = useState(false);
  const [credentialRef, setCredentialRef] = useState<string>(
    currentCredentialRef ?? credentials[0]?.name ?? "",
  );
  const [modelOverride, setModelOverride] = useState<string>(
    currentMode === PROVIDER_MODE.self_managed ? currentModel : "",
  );

  async function runPlatform(_prev: ActionState): Promise<ActionState> {
    const result = await resetProviderAction();
    if (!result.ok) return { ok: null, error: result.error };
    router.refresh();
    return { ok: PLATFORM_SUCCESS_MSG, error: null };
  }

  async function runSelfManaged(_prev: ActionState): Promise<ActionState> {
    const result = await setProviderSelfManagedAction({
      credential_ref: credentialRef,
      model: modelOverride.trim() || undefined,
    });
    if (!result.ok) return { ok: null, error: result.error };
    captureProductEvent(EVENTS.model_added, {
      provider: result.data.provider,
      mode: result.data.mode,
      model: result.data.model,
    });
    router.refresh();
    return { ok: SAVE_SUCCESS_MSG, error: null };
  }

  // The submit handler is the revealed view's: platform-switch when showing the
  // cards, self-managed save when the own-key form is open.
  async function action(prev: ActionState): Promise<ActionState> {
    return configuring ? runSelfManaged(prev) : runPlatform(prev);
  }

  const [state, submitAction, isPending] = useActionState(action, INITIAL_ACTION_STATE);

  const platformActive = currentMode === PROVIDER_MODE.platform;
  const ownKeyActive = currentMode === PROVIDER_MODE.self_managed;

  return (
    <ActionForm action={submitAction} className="space-y-4 text-sm">
      <div className="grid gap-4 lg:grid-cols-2">
        <OptionCard
          mode={PROVIDER_MODE.platform}
          title="Platform defaults"
          description="Built-in provider, paid per event from your balance. No key to manage."
          modelLine="▸ managed provider · default model · default context"
          isActive={platformActive}
          action={
            <Button type="submit" variant="outline" disabled={isPending}>
              {isPending && !configuring ? <Spinner size="sm" srLabel="Switching" /> : null}
              Use platform defaults
            </Button>
          }
        />
        <OptionCard
          mode={PROVIDER_MODE.self_managed}
          title="Bring your own key"
          description="Point the model config at any provider you hold a credential for."
          modelLine="▸ any provider · your model · your context"
          isActive={ownKeyActive}
          action={
            <Button type="button" variant="double-border" onClick={() => setConfiguring(true)}>
              Switch to own key
            </Button>
          }
        />
      </div>

      {configuring ? (
        <OwnKeyConfig
          workspaceId={workspaceId}
          credentials={credentials}
          catalogue={catalogue}
          credentialRef={credentialRef}
          modelOverride={modelOverride}
          isPending={isPending}
          onCredentialRefChange={setCredentialRef}
          onModelChange={setModelOverride}
          onCancel={() => setConfiguring(false)}
        />
      ) : null}

      <p className="text-xs text-muted-foreground">
        Changes apply to new events; events already in flight finish on their current configuration.
      </p>

      <ProviderSelectorFeedback state={state} />
    </ActionForm>
  );
}
