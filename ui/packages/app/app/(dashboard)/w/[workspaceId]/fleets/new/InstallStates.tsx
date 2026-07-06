"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import Link from "next/link";
import { Button } from "@agentsfleet/design-system";
import { EVENTS } from "@/lib/analytics/events";
import { captureProductEvent } from "@/lib/analytics/posthog";
import { FLEET_NAME_CONFLICT_MESSAGE } from "@/lib/errors";
import { WORKSPACE_SECRETS_PATH } from "@/lib/fleet-secrets";
import { workspacePath } from "@/lib/workspace-routes";
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
  // Optional operator-supplied fleet name. Absent ⇒ the library entry's
  // SKILL.md `name:` is used, so two installs of one library entry collide;
  // present ⇒ overrides it so one library entry can back several fleets in
  // the workspace.
  name?: string;
  onBack: () => void;
};

// One install experience, run inline. On mount it holds at the connect gate when
// a required credential is missing, then auto-proceeds to create — no confirm
// beat. After create it hands off to InstallStreamSteps, which advances the
// creating→provisioning→ready steps off the existing fleet-event stream and
// lands "Open fleet".
export function InstallStates({ workspaceId, source, presentCredentialNames, name, onBack }: Props) {
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
  const resolveCreateBody = useCallback((): Parameters<typeof installFleetAction>[1] => {
    const override = name?.trim();
    if (source.visibility === "platform") {
      return override
        ? { platform_library_id: source.id, name: override }
        : { platform_library_id: source.id };
    }
    return override
      ? { tenant_library_id: source.id, name: override }
      : { tenant_library_id: source.id };
  }, [source, name]);

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
    setFleet({ id: created.data.fleet_id, name: name?.trim() || requirements.name });
  }, [resolveCreateBody, workspaceId, requirements.name, name]);

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
// surfaced as a gate. Resolves via the custom-secret bridge — the one-click
// connector is a later milestone, so this links to the vault, not an app
// connect. There is no skip: a fleet that can't reach its tool can't run, so the
// only action is to connect. Back → re-enter re-evaluates the gate, and an
// operator returning with the credential stored auto-proceeds to create.
function ConnectGate({ workspaceId, unmet, reasons }: { workspaceId: string; unmet: string[]; reasons: Record<string, string> }) {
  const connectLabel = unmet.some((credential) => credential.toLowerCase().includes("github"))
    ? "Connect GitHub"
    : "Add token";
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
            Needs <span className="font-mono text-foreground">{unmet.join(", ")}</span>. Add{" "}
            {objectLabel} in Secrets to run this fleet.
          </>
        )}
      </p>
      <div className="flex flex-wrap items-center gap-2">
        <Button asChild size="sm">
          <Link href={workspacePath(workspaceId, WORKSPACE_SECRETS_PATH)}>{connectLabel}</Link>
        </Button>
      </div>
    </div>
  );
}
