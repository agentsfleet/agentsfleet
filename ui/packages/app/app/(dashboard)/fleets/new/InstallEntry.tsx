import Link from "next/link";
import { Button, EmptyState } from "@agentsfleet/design-system";
import { LayoutTemplateIcon, PlusIcon } from "lucide-react";
import type { FleetLibraryGalleryEntry } from "@/lib/types";
import {
  TemplateDocsLink,
  FLEET_LIBRARY_EMPTY_DESCRIPTION,
  FLEET_LIBRARY_EMPTY_DESCRIPTION_READONLY,
  FLEET_LIBRARY_EMPTY_TITLE,
} from "./template-docs";
import { TemplateCard } from "./TemplateCard";

type Props = {
  templates: FleetLibraryGalleryEntry[];
  /** Dashboard shows the primary cards; the full install page passes all. */
  maxTemplates?: number;
  /** Dashboard embed uses a denser card treatment. */
  compact?: boolean;
  /** Gates the Create-a-template affordance — mirrors InstallSourceSelector's
   * own gate so a viewer without library:write never sees an invitation to
   * do something the backend will reject. */
  canAddTemplate?: boolean;
};

// Template gallery for the Dashboard first-run surface. A Server Component:
// each card deep-links into the install page (which proceeds inline to live
// states), so it carries no client callbacks. When the catalogue is empty it
// falls back to a centered EmptyState with [Learn more] + [Create a template]
// — authoring itself lives on /fleets/new.
export function InstallEntry({
  templates,
  maxTemplates,
  compact = false,
  canAddTemplate = false,
}: Props) {
  const visibleTemplates =
    maxTemplates == null ? templates : templates.slice(0, maxTemplates);

  if (visibleTemplates.length === 0) {
    return (
      <EmptyState
        icon={<LayoutTemplateIcon size={28} />}
        title={FLEET_LIBRARY_EMPTY_TITLE}
        description={canAddTemplate ? FLEET_LIBRARY_EMPTY_DESCRIPTION : FLEET_LIBRARY_EMPTY_DESCRIPTION_READONLY}
        action={
          <div className="flex flex-wrap items-center justify-center gap-md">
            <TemplateDocsLink />
            {canAddTemplate ? (
              <Button asChild size="sm">
                <Link href="/fleets/new?create=1">
                  <PlusIcon size={14} /> Create a template
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
        {visibleTemplates.map((template) => (
          <TemplateCard
            key={template.id}
            template={template}
            compact={compact}
            action={
              <Button asChild>
                <Link href={`/fleets/new?template=${template.id}`}>Use template</Link>
              </Button>
            }
          />
        ))}
      </div>
    </div>
  );
}
