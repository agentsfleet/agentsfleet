"use client";

import { useEffect, useRef, useState } from "react";
import type { FleetTemplateGalleryEntry } from "@/lib/types";
import { InstallConfirm } from "./InstallConfirm";
import { InstallSourceSelector } from "./InstallSourceSelector";
import { InstallStates } from "./InstallStates";
import type { InstallSource } from "./install-flow";

type Props = {
  workspaceId: string;
  templates: FleetTemplateGalleryEntry[];
  presentCredentialNames: string[] | null;
  initialTemplateId?: string;
};

// Orchestrates the template-only install flow: pick a template from the gallery
// (platform ∪ this workspace's tenant templates), optionally name the fleet on
// the confirm step (so one template can back several fleets), then proceed inline
// to the live install states. Create auto-proceeds once the instant connect gate
// is satisfied. The states own connect → creating → done and land "Open fleet".
export function InstallFleet({
  workspaceId,
  templates,
  presentCredentialNames,
  initialTemplateId,
}: Props) {
  const [selection, setSelection] = useState<InstallSource | null>(null);
  // `null` ⇒ the operator has not confirmed the install yet (the confirm step is
  // showing); a string (possibly empty) ⇒ confirmed, carrying the optional name.
  const [installName, setInstallName] = useState<string | null>(null);

  // A ?template=<id> deep link (from the dashboard gallery) preselects the
  // template and lands on the confirm step on first render.
  const preselected = useRef(false);
  useEffect(() => {
    if (preselected.current || !initialTemplateId) return;
    preselected.current = true;
    const match = templates.find((template) => template.id === initialTemplateId);
    if (match) setSelection(match);
  }, [initialTemplateId, templates]);

  function reset() {
    setSelection(null);
    setInstallName(null);
  }

  if (selection && installName !== null) {
    return (
      <InstallStates
        workspaceId={workspaceId}
        source={selection}
        presentCredentialNames={presentCredentialNames}
        name={installName || undefined}
        onBack={reset}
      />
    );
  }

  if (selection) {
    return (
      <InstallConfirm
        template={selection}
        onInstall={(name) => setInstallName(name)}
        onBack={reset}
      />
    );
  }

  return (
    <InstallSourceSelector
      templates={templates}
      onUseTemplate={(template) => setSelection(template)}
    />
  );
}
