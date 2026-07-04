"use client";

import { Button, EmptyState, SectionLabel } from "@agentsfleet/design-system";
import { LayoutTemplateIcon } from "lucide-react";
import type { FleetLibraryGalleryEntry } from "@/lib/types";
import AddLibraryDialog from "./AddLibraryDialog";
import {
  LibraryDocsLink,
  FLEET_LIBRARY_EMPTY_DESCRIPTION,
  FLEET_LIBRARY_EMPTY_DESCRIPTION_READONLY,
  FLEET_LIBRARY_EMPTY_TITLE,
} from "./library-docs";
import { LibraryCard } from "./LibraryCard";

type Props = {
  workspaceId: string;
  entries: FleetLibraryGalleryEntry[];
  onUseLibraryEntry: (entry: FleetLibraryGalleryEntry) => void;
  canAddLibraryEntry?: boolean;
  /** Open the add-library-entry dialog on first render (?create=1 deep link). */
  initialCreateOpen?: boolean;
};

// Library gallery picker: the workspace's library entries (platform ∪ tenant)
// are the install surface. Picking one proceeds inline to the live install
// states — there is no review page. Rendered plainly under the page header
// (same shape as the dashboard's first-run gallery) — the page
// title/description already frame it, so no wrapping panel and no side guide.
export function InstallSourceSelector({
  workspaceId,
  entries,
  onUseLibraryEntry,
  canAddLibraryEntry = false,
  initialCreateOpen = false,
}: Props) {
  const showAddLibraryEntry = canAddLibraryEntry;
  return (
    <div className="space-y-sm">
      <div className="flex flex-wrap items-baseline justify-between gap-md">
        <SectionLabel>Fleet library</SectionLabel>
        {showAddLibraryEntry && entries.length > 0 ? (
          <AddLibraryDialog workspaceId={workspaceId} defaultOpen={initialCreateOpen} />
        ) : null}
      </div>
      {entries.length > 0 ? (
        <div className="grid grid-cols-1 gap-md sm:grid-cols-2 lg:grid-cols-3">
          {entries.map((entry) => (
            <LibraryCard
              key={entry.id}
              entry={entry}
              action={
                <Button type="button" onClick={() => onUseLibraryEntry(entry)}>
                  Use entry
                </Button>
              }
            />
          ))}
        </div>
      ) : (
        <EmptyState
          icon={<LayoutTemplateIcon size={28} />}
          title={FLEET_LIBRARY_EMPTY_TITLE}
          description={showAddLibraryEntry ? FLEET_LIBRARY_EMPTY_DESCRIPTION : FLEET_LIBRARY_EMPTY_DESCRIPTION_READONLY}
          action={
            <div className="flex flex-wrap items-center justify-center gap-md">
              <LibraryDocsLink />
              {showAddLibraryEntry ? (
                <AddLibraryDialog workspaceId={workspaceId} defaultOpen={initialCreateOpen} />
              ) : null}
            </div>
          }
        />
      )}
    </div>
  );
}
