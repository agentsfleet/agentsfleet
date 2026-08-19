"use client";

import { useState } from "react";
import type { FleetLibraryPageResult } from "@/lib/api/fleet-library";
import type { LibraryError } from "@/lib/api/library-types";
import type { FleetLibraryGalleryEntry } from "@/lib/types";
import { InstallSourceSelector } from "./InstallSourceSelector";
import { InstallStates } from "./InstallStates";
import type { InstallSource } from "./install-flow";

type Props = {
  workspaceId: string;
  /** First gallery page, or null when the read failed — see `initialError`. */
  initialPage: FleetLibraryPageResult | null;
  /** Typed read failure. Distinct from an empty library, and never both. */
  initialError: LibraryError | null;
  /**
   * Deep-link selection, already resolved on the server against the loaded
   * page. Passing the resolved ENTRY rather than an id is what removes the
   * gallery flash: there is no frame in which the gallery is correct.
   */
  initialSelection: FleetLibraryGalleryEntry | null;
  /** A `library_id` was asked for and is not on the loaded page. */
  selectionNotFound?: boolean;
  presentCredentialNames: string[] | null;
  canAddLibraryEntry?: boolean;
  /** Open the add-library-entry dialog on first render (?create=1 deep link). */
  initialCreateOpen?: boolean;
};

// Orchestrates the library-entry-only install flow: pick a library entry from
// the gallery (platform ∪ this workspace's tenant entries) and proceed straight
// to the live install states — one click, no confirm step. Naming is the
// server's job: an install that lands on a taken default name is auto-suffixed
// (`{template}-NNN`), and an explicit name stays a CLI affair
// (`agentsfleet install --library <id> --name`). The states own
// connect → creating → done and land "Open fleet".
export function InstallFleet({
  workspaceId,
  initialPage,
  initialError,
  initialSelection,
  selectionNotFound = false,
  presentCredentialNames,
  canAddLibraryEntry = false,
  initialCreateOpen = false,
}: Props) {
  // Seeded from the server-resolved selection. This used to start null and be
  // filled by an effect that matched `?library=<id>` against the entries after
  // hydration, so a deep link painted the gallery and then replaced it with the
  // install states a frame later. Seeding state directly removes that frame.
  const [selection, setSelection] = useState<InstallSource | null>(initialSelection);

  function reset() {
    setSelection(null);
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
      workspaceId={workspaceId}
      initialPage={initialPage}
      initialError={initialError}
      selectionNotFound={selectionNotFound}
      onUseLibraryEntry={(entry) => setSelection(entry)}
      canAddLibraryEntry={canAddLibraryEntry}
      initialCreateOpen={initialCreateOpen}
    />
  );
}
