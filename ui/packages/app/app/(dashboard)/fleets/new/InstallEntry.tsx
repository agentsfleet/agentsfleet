import Link from "next/link";
import { Button } from "@agentsfleet/design-system";
import type { FleetTemplateGalleryEntry } from "@/lib/types";
import { TemplateCard } from "./TemplateCard";

const QUICKSTART_URL = "https://docs.agentsfleet.net/quickstart";

type Props = {
  templates: FleetTemplateGalleryEntry[];
  // Appends a quickstart link below the gallery. The full install page is the
  // primary affordance; the dashboard embed omits it.
  quickstart?: boolean;
  /** Dashboard shows the primary cards; the full install page passes all. */
  maxTemplates?: number;
  /** Dashboard embed uses a denser card treatment. */
  compact?: boolean;
};

// Template entry surface for the Dashboard and install page. A Server Component:
// each card deep-links into the install page (which proceeds inline to live
// states), so it carries no client callbacks. github-import and paste authoring
// were removed in M103 — templates are the only install source.
export function InstallEntry({
  templates,
  quickstart = false,
  maxTemplates,
  compact = false,
}: Props) {
  const visibleTemplates =
    maxTemplates == null ? templates : templates.slice(0, maxTemplates);
  return (
    <div className={compact ? "space-y-md" : "space-y-lg"}>
      {visibleTemplates.length > 0 ? (
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
      ) : (
        <p className="text-body-sm leading-body-sm text-muted-foreground">
          No templates available yet.{" "}
          <Link href="/fleets/new" className="text-foreground underline underline-offset-4">
            Open the install page
          </Link>{" "}
          to browse, or onboard a template into your workspace.
        </p>
      )}

      {quickstart ? (
        <div className="flex flex-wrap gap-md border-t border-border pt-lg">
          <Button asChild variant="ghost" size="sm">
            <a href={QUICKSTART_URL} target="_blank" rel="noopener noreferrer">
              Quick start
            </a>
          </Button>
        </div>
      ) : null}
    </div>
  );
}
