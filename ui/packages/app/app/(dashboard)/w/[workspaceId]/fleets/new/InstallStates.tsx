"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import Link from "next/link";
import { Button } from "@agentsfleet/design-system";
import { EVENTS } from "@/lib/analytics/events";
import { captureProductEvent } from "@/lib/analytics/posthog";
import { FLEET_NAME_CONFLICT_MESSAGE } from "@/lib/errors";
import { WORKSPACE_SECRETS_PATH } from "@/lib/fleet-secrets";
import { WORKSPACE_INTEGRATIONS_PATH, workspacePath } from "@/lib/workspace-routes";
import { requestOnboardingRefresh } from "@/lib/onboarding-refresh";
import { installFleetAction } from "../actions";
import {
  flowError,
  readyToCreate,
  requirementsOf,
  STATE_GLYPH,
  unmetCredentials,
  type InstallSource,
  type StateLine,
} from "./install-flow";
import { InstallShell, StateList } from "./install-state-list";
import { InstallStreamSteps } from "./InstallStreamSteps";

type Props = {
  workspaceId: string;
  source: InstallSource;
  // The workspace's present credential names, or null when the vault read
  // failed — in which case the connect gate holds nothing back (the server's 424
  // stays authoritative).
  presentCredentialNames: string[] | null;
  // present ⇒ overrides it so one library entry can back several fleets in
  // the workspace.
  onBack: () => void;
};

// One install experience, run inline. On mount it holds at the connect gate when
// a required credential is missing, then auto-proceeds to create — no confirm
// beat. After create it hands off to InstallStreamSteps, which advances the
// creating→provisioning→ready steps off the existing fleet-event stream and
// lands "Open fleet".
export function InstallStates({ workspaceId, source, presentCredentialNames, onBack }: Props) {
  const router = useRouter();
  const requirements = requirementsOf(source);
  // Pre-create stages the flow drives directly. Post-create, InstallStreamSteps
  // owns the rendered steps (it reads the fleet event stream), so this component only
  // tracks up to the point a fleet exists. Initial stage is computed from the gate
  // so a ready library entry never flashes the connect copy before the effect runs.
  const [installStage, setInstallStage] = useState<"connect" | "creating" | "error">(() =>
    readyToCreate(requirements.credentials, presentCredentialNames) ? "creating" : "connect",
  );
  const [fleet, setFleet] = useState<{ id: string; name: string } | null>(null);
  const [errorText, setErrorText] = useState<string | null>(null);
  const started = useRef(false);

  // The create body keys off the entry's tier: a platform entry installs
  // by slug `platform_library_id`, a tenant entry by its UUID
  // `tenant_library_id`. No import step — the server reads SKILL/TRIGGER from
  // the onboarded library row.
  // No name rides the body: the dashboard installs one-step, and the server
  // auto-suffixes a taken default (`{template}-NNN`). An explicit name stays
  // the CLI's affair (`agentsfleet install --library <id> --name`).
  const resolveCreateBody = useCallback((): Parameters<typeof installFleetAction>[1] => {
    if (source.visibility === "platform") return { platform_library_id: source.id };
    return { tenant_library_id: source.id };
  }, [source]);

  const runCreate = useCallback(async () => {
    setInstallStage("creating");
    setErrorText(null);
    const created = await installFleetAction(workspaceId, resolveCreateBody());
    if (!created.ok) {
      setErrorText(
        created.status === 409 ? FLEET_NAME_CONFLICT_MESSAGE : flowError(created, "create the fleet"),
      );
      setInstallStage("error");
      return;
    }
    captureProductEvent(EVENTS.fleet_created, { fleet_id: created.data.fleet_id });
    requestOnboardingRefresh(workspaceId);
    setFleet({ id: created.data.fleet_id, name: created.data.name });
  }, [resolveCreateBody, workspaceId]);

  // Drive the flow once on mount: a source with no unmet credential creates
  // immediately; otherwise we sit on the connect gate until the operator
  // returns with the credential stored (Back → re-enter re-evaluates).
  useEffect(() => {
    if (started.current) return;
    started.current = true;
    if (readyToCreate(requirements.credentials, presentCredentialNames)) {
      void runCreate();
    } else {
      setInstallStage("connect");
    }
  }, [requirements.credentials, presentCredentialNames, runCreate]);

  // Once a fleet exists, the live event steps own the panel.
  if (fleet) {
    return (
      <InstallShell onBack={onBack} title={`installing · ${fleet.name}`}>
        <InstallStreamSteps
          workspaceId={workspaceId}
          fleetId={fleet.id}
          fleetName={fleet.name}
          onOpen={() => {
            router.push(workspacePath(workspaceId, `fleets/${fleet.id}`));
          }}
        />
      </InstallShell>
    );
  }

  const unmet = unmetCredentials(requirements.credentials, presentCredentialNames);
  return (
    <InstallShell onBack={onBack} title={`installing · ${requirements.name}`}>
      <PreCreateLines stage={installStage} requirements={requirements} unmet={unmet} errorText={errorText} />
      {installStage === "connect" ? (
        <ConnectGate workspaceId={workspaceId} unmet={unmet} reasons={requirements.credentialReasons} />
      ) : null}
      {installStage === "error" ? (
        <div className="border-t border-border px-lg py-md">
          <Button type="button" variant="ghost" size="sm" onClick={() => void runCreate()}>
            Retry
          </Button>
        </div>
      ) : null}
    </InstallShell>
  );
}

// ── pre-create state lines ─────────────────────────────────────────────────

function PreCreateLines({
  stage,
  requirements,
  unmet,
  errorText,
}: {
  stage: "connect" | "creating" | "error";
  requirements: ReturnType<typeof requirementsOf>;
  unmet: string[];
  errorText: string | null;
}) {
  const lines: StateLine[] = [];
  // No import step: the library entry is already onboarded, so the flow opens
  // on the selected library entry, then gates on credentials before create.
  lines.push({ id: "selected", tone: "ok", glyph: STATE_GLYPH.ok, text: `template · ${requirements.name}` });
  if (!requirements.triggerPresent) {
    lines.push({ id: "skill-only", tone: "wait", glyph: STATE_GLYPH.wait, text: "manual API wake will be generated" });
  }
  if (stage === "connect") {
    lines.push({ id: "connect", tone: "wait", glyph: STATE_GLYPH.wait, text: `first run: connect ${unmet.join(", ")}` });
  }
  if (stage === "creating") {
    lines.push({ id: "creating", tone: "run", glyph: STATE_GLYPH.run, text: "creating fleet…" });
  }
  if (stage === "error" && errorText) {
    lines.push({ id: "error", tone: "err", glyph: STATE_GLYPH.err, text: errorText });
  }
  return <StateList lines={lines} />;
}

// Connect gate: the requirement transparency the old review page showed,
// surfaced as a gate. GitHub is a connector and resolves on Integrations;
// custom secrets still resolve in the vault. There is no skip: a fleet that
// cannot reach its tool cannot run. Back → re-enter re-evaluates the gate, and
// a returning operator with the credential stored auto-proceeds to create.
function ConnectGate({ workspaceId, unmet, reasons }: { workspaceId: string; unmet: string[]; reasons: Record<string, string> }) {
  const githubOnly = unmet.length === 1 && unmet[0]?.toLowerCase() === "github";
  const connectLabel = githubOnly ? "Connect" : "Add token";
  const connectPath = githubOnly ? WORKSPACE_INTEGRATIONS_PATH : WORKSPACE_SECRETS_PATH;
  const objectLabel = unmet.length === 1 ? "it" : "them";
  // Purpose-driven copy when the library entry declares why each credential is needed
  // (e.g. "to review your pull requests"); otherwise the generic connect prompt.
  // Only when EVERY unmet credential has a reason, so the sentence never lists a
  // credential whose purpose is missing.
  const purposes = unmet.map((credential) => reasons[credential]).filter(Boolean);
  const allHaveReasons = unmet.length > 0 && purposes.length === unmet.length;
  return (
    <div className="space-y-3 border-t border-border px-lg py-md">
      <p className="text-sm text-muted-foreground">
        {allHaveReasons ? (
          <>
            This fleet needs <span className="font-mono text-foreground">{unmet.join(", ")}</span> to{" "}
            {purposes.join(" and ")}.
          </>
        ) : (
          <>
            Needs <span className="font-mono text-foreground">{unmet.join(", ")}</span>.{" "}
            {githubOnly
              ? `Connect ${objectLabel} in Integrations to run this fleet.`
              : `Add ${objectLabel} in Secrets to run this fleet.`}
          </>
        )}
      </p>
      <div className="flex flex-wrap items-center gap-2">
        <Button asChild size="sm">
          <Link href={workspacePath(workspaceId, connectPath)}>{connectLabel}</Link>
        </Button>
      </div>
    </div>
  );
}
