import Link from "next/link";
import { Button, EmptyState } from "@agentsfleet/design-system";
import { LayoutTemplateIcon } from "lucide-react";
import type { FleetTemplateGalleryEntry } from "@/lib/types";
import { CreateTemplateDocLink } from "./template-docs";
import { TemplateCard } from "./TemplateCard";

type Props = {
  templates: FleetTemplateGalleryEntry[];
  /** Dashboard shows the primary cards; the full install page passes all. */
  maxTemplates?: number;
  /** Dashboard embed uses a denser card treatment. */
  compact?: boolean;
};

// Template gallery for the Dashboard first-run card. A Server Component: each
// card deep-links into the install page (which proceeds inline to live states),
// so it carries no client callbacks. When the catalogue is empty it falls back
// to a centered EmptyState with a prominent "Create a template" affordance —
// the "add template" / paste authoring surfaces live on /fleets/new.
export function InstallEntry({ templates, maxTemplates, compact = false }: Props) {
  const visibleTemplates =
    maxTemplates == null ? templates : templates.slice(0, maxTemplates);

  if (visibleTemplates.length === 0) {
    return (
      <EmptyState
        icon={<LayoutTemplateIcon size={28} />}
        title="No templates found"
        description="Write your own template to install your first fleet."
        action={<CreateTemplateDocLink variant="default" />}
      />
    );
  }

  return (
    <div className={compact ? "space-y-md" : "space-y-lg"}>
      <div className="grid grid-cols-1 gap-md sm:grid-cols-2">
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
