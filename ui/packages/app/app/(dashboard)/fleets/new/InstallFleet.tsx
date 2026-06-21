"use client";

import { useActionState, useEffect, useRef, useState, useTransition } from "react";
import { useRouter } from "next/navigation";
import { Button } from "@agentsfleet/design-system";
import type { BundleSnapshot, FleetTemplate } from "@/lib/types";
import { importBundleAction, installFleetAction } from "../actions";
import { FLEET_NAME_CONFLICT_MESSAGE, presentErrorString } from "@/lib/errors";
import { EVENTS } from "@/lib/analytics/events";
import { captureProductEvent } from "@/lib/analytics/posthog";
import { InstallSourceSelector } from "./InstallSourceSelector";
import { BundlePreview } from "./BundlePreview";
import InstallFleetForm from "./InstallFleetForm";

type BundleSource =
  | { kind: "template"; template: FleetTemplate }
  | { kind: "github"; snapshot: BundleSnapshot };

type Selection = BundleSource | { kind: "paste" } | null;

type ImportState = { error: string | null };

// owner/repo: exactly one slash, non-empty whitespace-free parts. Mirrors the
// server-side parse (`resolve.zig`) so the client rejects the same shapes the
// server would (`owner/`, `/repo`, `owner/repo/extra`) before a round-trip.
const OWNER_REPO_PATTERN = /^[^/\s]+\/[^/\s]+$/;

type Props = {
  workspaceId: string;
  templates: FleetTemplate[];
  presentCredentialNames: string[] | null;
  initialTemplateId?: string;
};

// Orchestrates the gallery-first install flow: pick a source (template / GitHub /
// paste), preview what it needs, then create. Templates preview from catalog
// metadata (no fetch); GitHub previews from the imported snapshot. Create reuses
// the existing install handler with `{ bundle_id }`.
export function InstallFleet({
  workspaceId,
  templates,
  presentCredentialNames,
  initialTemplateId,
}: Props) {
  const router = useRouter();
  const [selection, setSelection] = useState<Selection>(null);
  const [createError, setCreateError] = useState<string | null>(null);
  const [creating, startCreate] = useTransition();

  const [importState, runImport, importPending] = useActionState(
    async (_prev: ImportState, sourceRef: string): Promise<ImportState> => {
      const ref = sourceRef.trim();
      if (!OWNER_REPO_PATTERN.test(ref)) {
        return { error: "Enter a GitHub repository as owner/repo." };
      }
      const result = await importBundleAction(workspaceId, {
        source_kind: "github",
        source_ref: ref,
      });
      if (!result.ok) {
        return { error: actionError(result, "import the repository") };
      }
      setSelection({ kind: "github", snapshot: result.data });
      return { error: null };
    },
    { error: null },
  );

  // A ?template=<id> deep link (from the dashboard gallery) preselects that
  // template's preview on first render.
  const preselected = useRef(false);
  useEffect(() => {
    if (preselected.current || !initialTemplateId) return;
    preselected.current = true;
    const match = templates.find((template) => template.id === initialTemplateId);
    if (match) setSelection({ kind: "template", template: match });
  }, [initialTemplateId, templates]);

  function reset() {
    setCreateError(null);
    setSelection(null);
  }

  if (selection?.kind === "paste") {
    return (
      <div className="space-y-4">
        <Button type="button" variant="ghost" size="sm" onClick={reset}>
          ← Back to templates
        </Button>
        <InstallFleetForm workspaceId={workspaceId} />
      </div>
    );
  }

  if (selection) {
    const source = selection;
    const preview = previewFacts(source);
    const onCreate = (nameOverride?: string) => {
      setCreateError(null);
      startCreate(async () => {
        const resolved = await resolveBundleId(workspaceId, source);
        if (!resolved.ok) {
          setCreateError(resolved.error);
          return;
        }
        const created = await installFleetAction(workspaceId, {
          bundle_id: resolved.value,
          name: nameOverride,
        });
        if (!created.ok) {
          setCreateError(
            created.status === 409
              ? FLEET_NAME_CONFLICT_MESSAGE
              : actionError(created, "create the teammate"),
          );
          return;
        }
        captureProductEvent(EVENTS.fleet_created, { fleet_id: created.data.fleet_id });
        router.push(`/fleets/${created.data.fleet_id}`);
      });
    };
    return (
      <BundlePreview
        name={preview.name}
        credentials={preview.credentials}
        tools={preview.tools}
        networkHosts={preview.networkHosts}
        presentCredentialNames={presentCredentialNames}
        defaultFleetName={preview.defaultFleetName}
        creating={creating}
        createError={createError}
        onCreate={onCreate}
        onBack={reset}
      />
    );
  }

  return (
    <InstallSourceSelector
      templates={templates}
      onUseTemplate={(template) => setSelection({ kind: "template", template })}
      onImport={runImport}
      importPending={importPending}
      importError={importState.error}
      onPaste={() => setSelection({ kind: "paste" })}
    />
  );
}

// Resolves the bundle_id to create from. GitHub sources are already imported;
// templates import lazily at create time (their content is fetched server-side,
// so an unpopulated template repo surfaces its import error here).
async function resolveBundleId(
  workspaceId: string,
  source: BundleSource,
): Promise<{ ok: true; value: string } | { ok: false; error: string }> {
  if (source.kind === "github") {
    return { ok: true, value: source.snapshot.bundle_id };
  }
  const imported = await importBundleAction(workspaceId, {
    source_kind: "template",
    source_ref: source.template.id,
  });
  if (!imported.ok) {
    return { ok: false, error: actionError(imported, "import the template") };
  }
  return { ok: true, value: imported.data.bundle_id };
}

// Renders a failed server action into a friendly message, threading the action
// label ("import the template", "create the teammate") through the shared
// error presenter so every install failure reads consistently.
function actionError(result: { errorCode?: string; error: string }, action: string): string {
  return presentErrorString({ errorCode: result.errorCode, message: result.error, action });
}

// Normalizes the preview shape across a template (catalog metadata) and a
// GitHub snapshot (parsed requirements).
function previewFacts(source: BundleSource): {
  name: string;
  credentials: string[];
  tools: string[];
  networkHosts: string[];
  defaultFleetName?: string;
} {
  if (source.kind === "template") {
    const { template } = source;
    // No defaultFleetName: a template's real SKILL.md name resolves server-side
    // at import, so the Name field stays blank (placeholder-guided) until then.
    return {
      name: template.name,
      credentials: template.required_credentials,
      tools: template.required_tools,
      networkHosts: template.network_hosts,
    };
  }
  const { snapshot } = source;
  return {
    name: snapshot.name,
    credentials: snapshot.requirements.credentials,
    tools: snapshot.requirements.tools,
    networkHosts: snapshot.requirements.network_hosts,
    defaultFleetName: snapshot.name,
  };
}
