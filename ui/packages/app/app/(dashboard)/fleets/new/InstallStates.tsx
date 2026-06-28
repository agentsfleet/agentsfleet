"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import Link from "next/link";
import { Button } from "@agentsfleet/design-system";
import { EVENTS } from "@/lib/analytics/events";
import { captureProductEvent } from "@/lib/analytics/posthog";
import { FLEET_NAME_CONFLICT_MESSAGE } from "@/lib/errors";
import { WORKSPACE_CREDENTIALS_PATH } from "@/lib/fleet-credentials";
import { importBundleAction, installFleetAction } from "../actions";
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
  // failed — in which case connect-to-continue gates nothing (the server's 424
  // stays authoritative).
  presentCredentialNames: string[] | null;
  onBack: () => void;
};

// One install experience, run inline. On mount it imports (when the source
// needs it), gates on connect-to-continue when a required credential is
// missing, then auto-proceeds to create — no confirm beat. After create it
// hands off to InstallStreamSteps, which advances the creating→provisioning→
// ready steps off the existing fleet-event stream and lands "Open fleet".
export function InstallStates({ workspaceId, source, presentCredentialNames, onBack }: Props) {
  const router = useRouter();
  const requirements = requirementsOf(source);
  // Pre-create stages the flow drives directly. Post-create, InstallStreamSteps
  // owns the rendered steps (it reads the fleet event stream), so this component only
  // tracks up to the point a fleet exists.
  const [installStage, setInstallStage] = useState<"importing" | "connect" | "creating" | "error">("importing");
  const [fleet, setFleet] = useState<{ id: string; name: string } | null>(null);
  const [errorText, setErrorText] = useState<string | null>(null);
  const started = useRef(false);

  // Resolve the bundle_id to create from. A GitHub source is already imported;
  // a template imports lazily here (its content is fetched server-side, so an
  // unpopulated template repo surfaces its import error at this step).
  const resolveCreateBody = useCallback(async (): Promise<
    | { ok: true; body: Parameters<typeof installFleetAction>[1] }
    | { ok: false; error: string }
  > => {
    if (source.kind === "paste") {
      const body = source.triggerMarkdown
        ? { source_markdown: source.sourceMarkdown, trigger_markdown: source.triggerMarkdown }
        : { source_markdown: source.sourceMarkdown };
      return { ok: true, body };
    }
    if (source.kind === "github") {
      return { ok: true, body: { bundle_id: source.snapshot.bundle_id } };
    }
    const imported = await importBundleAction(workspaceId, {
      source_kind: "template",
      source_ref: source.template.id,
    });
    if (!imported.ok) return { ok: false, error: flowError(imported, "import the template") };
    return { ok: true, body: { bundle_id: imported.data.bundle_id } };
  }, [source, workspaceId]);

  const runCreate = useCallback(async () => {
    setInstallStage("creating");
    setErrorText(null);
    const resolved = await resolveCreateBody();
    if (!resolved.ok) {
      setErrorText(resolved.error);
      setInstallStage("error");
      return;
    }
    const created = await installFleetAction(workspaceId, resolved.body);
    if (!created.ok) {
      setErrorText(
        created.status === 409 ? FLEET_NAME_CONFLICT_MESSAGE : flowError(created, "create the fleet"),
      );
      setInstallStage("error");
      return;
    }
    captureProductEvent(EVENTS.fleet_created, { fleet_id: created.data.fleet_id });
    setFleet({ id: created.data.fleet_id, name: requirements.name });
  }, [resolveCreateBody, workspaceId, requirements.name]);

  // Drive the flow once on mount: a source with no unmet credential creates
  // immediately; otherwise we sit on connect-to-continue until the operator
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
            router.push(`/fleets/${fleet.id}`);
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
        <ConnectGate unmet={unmet} />
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
  stage: "importing" | "connect" | "creating" | "error";
  requirements: ReturnType<typeof requirementsOf>;
  unmet: string[];
  errorText: string | null;
}) {
  const lines: StateLine[] = [];
  if (stage === "importing") {
    lines.push({ id: "importing", tone: "run", glyph: STATE_GLYPH.run, text: `importing ${requirements.name}…` });
  } else {
    lines.push({ id: "imported", tone: "ok", glyph: STATE_GLYPH.ok, text: `imported ${requirements.name}` });
  }
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
function ConnectGate({ unmet }: { unmet: string[] }) {
  const connectLabel = unmet.some((credential) => credential.toLowerCase().includes("github"))
    ? "Connect GitHub"
    : "Add token";
  const objectLabel = unmet.length === 1 ? "it" : "them";
  return (
    <div className="space-y-3 border-t border-border px-lg py-md">
      <p className="text-sm text-muted-foreground">
        Needs <span className="font-mono text-foreground">{unmet.join(", ")}</span>. Add{" "}
        {objectLabel} in Credentials to run this fleet.
      </p>
      <div className="flex flex-wrap items-center gap-2">
        <Button asChild size="sm">
          <Link href={WORKSPACE_CREDENTIALS_PATH}>{connectLabel}</Link>
        </Button>
      </div>
    </div>
  );
}
