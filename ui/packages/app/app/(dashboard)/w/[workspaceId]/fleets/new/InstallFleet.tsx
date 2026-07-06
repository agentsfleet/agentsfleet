"use client";

import { useEffect, useRef, useState } from "react";
import type { FleetLibraryGalleryEntry } from "@/lib/types";
import { InstallConfirm } from "./InstallConfirm";
import { InstallSourceSelector } from "./InstallSourceSelector";
import { InstallStates } from "./InstallStates";
import type { InstallSource } from "./install-flow";

type Props = {
  workspaceId: string;
  entries: FleetLibraryGalleryEntry[];
  presentCredentialNames: string[] | null;
  initialLibraryId?: string;
  canAddLibraryEntry?: boolean;
  /** Open the add-library-entry dialog on first render (?create=1 deep link). */
  initialCreateOpen?: boolean;
};

// Orchestrates the library-entry-only install flow: pick a library entry from
// the gallery (platform ∪ this workspace's tenant entries), optionally name
// the fleet on the confirm step (so one library entry can back several
// fleets), then proceed inline to the live install states. Create
// auto-proceeds once the instant connect gate is satisfied. The states own
// connect → creating → done and land "Open fleet".
export function InstallFleet({
  workspaceId,
  entries,
  presentCredentialNames,
  initialLibraryId,
  canAddLibraryEntry = false,
  initialCreateOpen = false,
}: Props) {
  const [selection, setSelection] = useState<InstallSource | null>(null);
  // `null` ⇒ the operator has not confirmed the install yet (the confirm step is
  // showing); a string (possibly empty) ⇒ confirmed, carrying the optional name.
  const [installName, setInstallName] = useState<string | null>(null);

  // A ?library=<id> deep link (from the dashboard gallery) preselects the
  // library entry and lands on the confirm step on first render.
  const preselected = useRef(false);
  useEffect(() => {
    if (preselected.current || !initialLibraryId) return;
    preselected.current = true;
    const match = entries.find((entry) => entry.id === initialLibraryId);
    if (match) setSelection(match);
  }, [initialLibraryId, entries]);

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
        entry={selection}
        onInstall={(name) => setInstallName(name)}
        onBack={reset}
      />
    );
  }

  return (
    <InstallSourceSelector
      workspaceId={workspaceId}
      entries={entries}
      onUseLibraryEntry={(entry) => setSelection(entry)}
      canAddLibraryEntry={canAddLibraryEntry}
      initialCreateOpen={initialCreateOpen}
    />
  );
}
