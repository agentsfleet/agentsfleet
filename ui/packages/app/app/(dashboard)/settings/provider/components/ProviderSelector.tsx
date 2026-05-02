"use client";

import { useActionState, useState } from "react";
import { useRouter } from "next/navigation";
import { Loader2Icon } from "lucide-react";
import { Alert, Button } from "@usezombie/design-system";
import { useClientToken } from "@/lib/auth/client";
import { resetTenantProvider, setTenantProviderByok } from "@/lib/api/tenant_provider";
import type { CredentialSummary } from "@/lib/api/credentials";
import { PROVIDER_MODE, type ProviderMode } from "@/lib/types";
import ModeRadio from "./ModeRadio";
import ByokFields from "./ByokFields";

type Props = {
  workspaceId: string;
  currentMode: ProviderMode;
  currentCredentialRef: string | null;
  currentModel: string;
  credentials: CredentialSummary[];
};

type ActionState = { ok: string | null; error: string | null };

type ModeStrategy = {
  submitLabel: string;
  successMsg: string;
  run: (token: string, form: { credentialRef: string; modelOverride: string }) => Promise<unknown>;
};

const MODE_STRATEGIES: Record<ProviderMode, ModeStrategy> = {
  platform: {
    submitLabel: "Reset to platform default",
    successMsg: "Reset to platform default.",
    run: (token) => resetTenantProvider(token),
  },
  byok: {
    submitLabel: "Save BYOK config",
    successMsg: "Switched to BYOK. Run a test event to verify the key.",
    run: (token, { credentialRef, modelOverride }) =>
      setTenantProviderByok(
        { credential_ref: credentialRef, model: modelOverride.trim() || undefined },
        token,
      ),
  },
};

const INITIAL_ACTION_STATE: ActionState = { ok: null, error: null };

export default function ProviderSelector({
  workspaceId,
  currentMode,
  currentCredentialRef,
  currentModel,
  credentials,
}: Props) {
  const router = useRouter();
  const { getToken } = useClientToken();

  // Form-controlled inputs are local state; the action below is the React 19
  // form-action handler that the <form> submits to.
  const [mode, setMode] = useState<ProviderMode>(currentMode);
  const [credentialRef, setCredentialRef] = useState<string>(
    currentCredentialRef ?? credentials[0]?.name ?? "",
  );
  const [modelOverride, setModelOverride] = useState<string>(
    currentMode === PROVIDER_MODE.byok ? currentModel : "",
  );

  const isByok = mode === PROVIDER_MODE.byok;
  const noCredentials = credentials.length === 0;

  const strategy = MODE_STRATEGIES[mode];

  async function action(_prev: ActionState, _formData: FormData): Promise<ActionState> {
    const token = await getToken();
    if (!token) return { ok: null, error: "Not authenticated" };
    try {
      await strategy.run(token, { credentialRef, modelOverride });
      router.refresh();
      return { ok: strategy.successMsg, error: null };
    } catch (err) {
      return { ok: null, error: err instanceof Error ? err.message : String(err) };
    }
  }

  const [state, submitAction, isPending] = useActionState(action, INITIAL_ACTION_STATE);

  return (
    <form action={submitAction} className="mt-3 space-y-5 text-sm">
      <fieldset className="space-y-2">
        <legend className="mb-2 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
          Mode
        </legend>
        <ModeRadio
          value={PROVIDER_MODE.platform}
          checked={mode === PROVIDER_MODE.platform}
          onChange={() => setMode(PROVIDER_MODE.platform)}
          label="Platform-managed"
          description="Zombie credits cover everything. Charged from your tenant balance per event."
        />
        <ModeRadio
          value={PROVIDER_MODE.byok}
          checked={isByok}
          onChange={() => setMode(PROVIDER_MODE.byok)}
          label="Bring your own key"
          description="Your provider account, your API key. We charge a flat per-event overhead."
        />
      </fieldset>

      {isByok ? (
        <ByokFields
          workspaceId={workspaceId}
          credentials={credentials}
          credentialRef={credentialRef}
          onCredentialRefChange={setCredentialRef}
          modelOverride={modelOverride}
          onModelOverrideChange={setModelOverride}
        />
      ) : null}

      <div className="flex items-center gap-3">
        <Button type="submit" disabled={isPending || (isByok && noCredentials)}>
          {isPending ? <Loader2Icon size={14} className="animate-spin" aria-hidden /> : null}
          {strategy.submitLabel}
        </Button>
        {state.ok ? (
          <span
            role="status"
            className="text-xs text-success animate-in fade-in-0 slide-in-from-bottom-1 duration-200"
          >
            {state.ok}
          </span>
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
