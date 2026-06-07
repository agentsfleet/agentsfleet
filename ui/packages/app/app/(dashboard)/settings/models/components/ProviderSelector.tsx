"use client";

import { useActionState, useState } from "react";
import { useRouter } from "next/navigation";
import { Alert, Badge, Button, RadioGroup, Spinner } from "@usezombie/design-system";
import { resetProviderAction, setProviderSelfManagedAction } from "../actions";
import type { ActionResult } from "@/lib/actions/with-token";
import type { CredentialSummary } from "@/lib/api/credentials";
import type { ModelCap } from "@/lib/api/model_caps";
import { PROVIDER_MODE, type ProviderMode, type TenantProvider } from "@/lib/types";
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
    submitLabel: "Reset to platform default",
    successMsg: "Reset to platform default.",
    run: () => resetProviderAction(),
  },
  self_managed: {
    submitLabel: "Save self-managed key",
    successMsg: "Switched to self-managed. Run a test event to verify the key.",
    run: ({ credentialRef, modelOverride }) =>
      setProviderSelfManagedAction({
        credential_ref: credentialRef,
        model: modelOverride.trim() || undefined,
      }),
  },
};

const INITIAL_ACTION_STATE: ActionState = { ok: null, error: null };

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
  // form-action handler that the <form> submits to.
  const [mode, setMode] = useState<ProviderMode>(currentMode);
  const [credentialRef, setCredentialRef] = useState<string>(
    currentCredentialRef ?? credentials[0]?.name ?? "",
  );
  const [modelOverride, setModelOverride] = useState<string>(
    currentMode === PROVIDER_MODE.self_managed ? currentModel : "",
  );

  const isSelfManaged = mode === PROVIDER_MODE.self_managed;

  const strategy = MODE_STRATEGIES[mode];

  async function action(_prev: ActionState, _formData: FormData): Promise<ActionState> {
    const result = await strategy.run({ credentialRef, modelOverride });
    if (!result.ok) return { ok: null, error: result.error };
    router.refresh();
    return { ok: strategy.successMsg, error: null };
  }

  const [state, submitAction, isPending] = useActionState(action, INITIAL_ACTION_STATE);

  return (
    <form action={submitAction} className="mt-3 space-y-5 text-sm">
      <fieldset className="space-y-2">
        <legend className="mb-2 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
          Mode
        </legend>
        <RadioGroup
          value={mode}
          onValueChange={(v) => setMode(v as ProviderMode)}
          aria-label="Provider mode"
        >
          <ModeRadio
            value={PROVIDER_MODE.platform}
            checked={mode === PROVIDER_MODE.platform}
            label="Platform-managed"
            description="Agent credits cover everything. Charged from your tenant balance per event."
          />
          <ModeRadio
            value={PROVIDER_MODE.self_managed}
            checked={isSelfManaged}
            label="Use my own provider key"
            description="Your provider account, your API key. We charge a flat per-event overhead."
          />
        </RadioGroup>
      </fieldset>

      {isSelfManaged ? (
        <div className="space-y-4 border-l-2 border-border pl-4 animate-in fade-in-0 slide-in-from-top-2 duration-300 ease-out">
          <Step1Credential
            workspaceId={workspaceId}
            credentials={credentials}
            credentialRef={credentialRef}
            onCredentialRefChange={setCredentialRef}
          />
          <Step2Model catalogue={catalogue} model={modelOverride} onModelChange={setModelOverride} />
        </div>
      ) : null}

      <div className="flex items-center gap-3">
        <Button type="submit" disabled={isPending || (isSelfManaged && credentialRef === "")}>
          {isPending ? <Spinner size="sm" srLabel="Saving" /> : null}
          {strategy.submitLabel}
        </Button>
        {state.ok ? (
          <Badge
            variant="green"
            role="status" // oxlint-disable-line jsx-a11y/prefer-tag-over-role -- Badge is the design-system primitive; <output> drops text children in happy-dom@20.
            className="animate-in fade-in-0 slide-in-from-bottom-1 duration-200 normal-case tracking-normal"
          >
            {state.ok}
          </Badge>
        ) : null}
      </div>

      {state.error ? (
        <Alert variant="destructive" className="text-xs">
          {state.error}
        </Alert>
      ) : null}
    </form>
  );
}
