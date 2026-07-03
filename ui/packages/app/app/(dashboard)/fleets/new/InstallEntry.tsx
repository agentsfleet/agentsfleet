import Link from "next/link";
import { Button, EmptyState } from "@agentsfleet/design-system";
import { LayoutTemplateIcon, PlusIcon } from "lucide-react";
import type { FleetTemplateGalleryEntry } from "@/lib/types";
import {
  TemplateDocsLink,
  TEMPLATES_EMPTY_DESCRIPTION,
  TEMPLATES_EMPTY_TITLE,
} from "./template-docs";
import { TemplateCard } from "./TemplateCard";

type Props = {
  templates: FleetTemplateGalleryEntry[];
  /** Dashboard shows the primary cards; the full install page passes all. */
  maxTemplates?: number;
  /** Dashboard embed uses a denser card treatment. */
  compact?: boolean;
};

// Template gallery for the Dashboard first-run surface. A Server Component:
// each card deep-links into the install page (which proceeds inline to live
// states), so it carries no client callbacks. When the catalogue is empty it
// falls back to a centered EmptyState with [Learn more] + [Create a template]
// — authoring itself lives on /fleets/new.
export function InstallEntry({ templates, maxTemplates, compact = false }: Props) {
  const visibleTemplates =
    maxTemplates == null ? templates : templates.slice(0, maxTemplates);

  if (visibleTemplates.length === 0) {
    return (
      <EmptyState
        icon={<LayoutTemplateIcon size={28} />}
        title={TEMPLATES_EMPTY_TITLE}
        description={TEMPLATES_EMPTY_DESCRIPTION}
        action={
          <div className="flex flex-wrap items-center justify-center gap-md">
            <TemplateDocsLink />
            <Button asChild size="sm">
              <Link href="/fleets/new?create=1">
                <PlusIcon size={14} /> Create a template
              </Link>
            </Button>
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
