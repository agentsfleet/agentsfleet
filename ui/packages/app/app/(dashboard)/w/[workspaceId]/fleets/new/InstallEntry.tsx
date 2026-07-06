import Link from "next/link";
import { Button, EmptyState } from "@agentsfleet/design-system";
import { LayoutTemplateIcon, PlusIcon } from "lucide-react";
import type { FleetLibraryGalleryEntry } from "@/lib/types";
import { workspacePath } from "@/lib/workspace-routes";
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
  /** Dashboard shows the primary cards; the full install page passes all. */
  maxEntries?: number;
  /** Dashboard embed uses a denser card treatment. */
  compact?: boolean;
  /** Gates the Add-library-entry affordance — mirrors InstallSourceSelector's
   * own gate so a viewer without library:write never sees an invitation to
   * do something the backend will reject. */
  canAddLibraryEntry?: boolean;
};

// Library gallery for the Dashboard first-run surface. A Server Component:
// each card deep-links into the install page (which proceeds inline to live
// states), so it carries no client callbacks. When the catalogue is empty it
// falls back to a centered EmptyState with [Learn more] + [Create fleet library]
// — authoring itself lives on /fleets/new.
export function InstallEntry({
  workspaceId,
  entries,
  maxEntries,
  compact = false,
  canAddLibraryEntry = false,
}: Props) {
  const visibleEntries =
    maxEntries == null ? entries : entries.slice(0, maxEntries);

  if (visibleEntries.length === 0) {
    return (
      <EmptyState
        icon={<LayoutTemplateIcon size={28} />}
        title={FLEET_LIBRARY_EMPTY_TITLE}
        description={canAddLibraryEntry ? FLEET_LIBRARY_EMPTY_DESCRIPTION : FLEET_LIBRARY_EMPTY_DESCRIPTION_READONLY}
        action={
          <div className="flex flex-wrap items-center justify-center gap-md">
            <LibraryDocsLink />
            {canAddLibraryEntry ? (
              <Button asChild size="sm">
                <Link href={`${workspacePath(workspaceId, "fleets/new")}?create=1`}>
                  <PlusIcon size={14} /> Create fleet library
                </Link>
              </Button>
            ) : null}
          </div>
        }
      />
    );
  }

  return (
    <div className={compact ? "space-y-md" : "space-y-lg"}>
      <div className="grid grid-cols-1 gap-md sm:grid-cols-2 lg:grid-cols-3">
        {visibleEntries.map((entry) => (
          <LibraryCard
            key={entry.id}
            entry={entry}
            compact={compact}
            action={
              <Button asChild>
                <Link href={`${workspacePath(workspaceId, "fleets/new")}?library=${entry.id}`}>Use entry</Link>
              </Button>
            }
          />
        ))}
      </div>
    </div>
  );
}
