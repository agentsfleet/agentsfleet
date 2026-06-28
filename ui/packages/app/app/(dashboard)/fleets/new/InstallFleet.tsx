"use client";

import { useActionState, useEffect, useRef, useState } from "react";
import type { BundleSnapshot, FleetTemplate } from "@/lib/types";
import { importBundleAction } from "../actions";
import { InstallSourceSelector } from "./InstallSourceSelector";
import InstallFleetForm from "./InstallFleetForm";
import { InstallStates } from "./InstallStates";
import { flowError, type InstallSource } from "./install-flow";

type Selection = InstallSource | { kind: "paste-input" } | null;

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

// Orchestrates the minimal, state-driven install flow: pick a source
// (template / GitHub / paste), then proceed INLINE to the live install states —
// never to a separate review/preview page. Create auto-proceeds once the source
// resolves (and the instant connect gate is satisfied). The states own
// importing → connect → creating → done and land "Open fleet".
export function InstallFleet({
  workspaceId,
  templates,
  presentCredentialNames,
  initialTemplateId,
}: Props) {
  const [selection, setSelection] = useState<Selection>(null);

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
        return { error: flowError(result, "import the repository") };
      }
      setSelection({ kind: "github", snapshot: result.data as BundleSnapshot });
      return { error: null };
    },
    { error: null },
  );

  // A ?template=<id> deep link (from the dashboard gallery) proceeds straight
  // into that template's install states on first render.
  const preselected = useRef(false);
  useEffect(() => {
    if (preselected.current || !initialTemplateId) return;
    preselected.current = true;
    const match = templates.find((template) => template.id === initialTemplateId);
    if (match) setSelection({ kind: "template", template: match });
  }, [initialTemplateId, templates]);

  function reset() {
    setSelection(null);
  }

  // The paste input — validates the markdown, then hands a `paste` source to the
  // states so create runs inline (no direct post, no route), keeping paste on
  // the same one-experience path as templates and GitHub.
  if (selection?.kind === "paste-input") {
    return (
      <InstallFleetForm
        onBack={reset}
        onSubmit={(sourceMarkdown, triggerMarkdown) =>
          setSelection({ kind: "paste", sourceMarkdown, triggerMarkdown })
        }
      />
    );
  }

  if (selection) {
    return (
      <InstallStates
        workspaceId={workspaceId}
        source={selection}
        presentCredentialNames={presentCredentialNames}
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
      onPaste={() => setSelection({ kind: "paste-input" })}
    />
  );
}
