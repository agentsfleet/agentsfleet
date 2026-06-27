import Link from "next/link";
import { Button } from "@agentsfleet/design-system";
import type { FleetTemplate } from "@/lib/types";
import { TemplateCard } from "./TemplateCard";

const QUICKSTART_URL = "https://docs.agentsfleet.net/quickstart";

type Props = {
  templates: FleetTemplate[];
  // Optional trailing actions appended to the source strip. The dashboard hides
  // this strip because import/paste belongs on the full install page.
  quickstart?: boolean;
  /** Dashboard shows the primary cards; the full install page passes all. */
  maxTemplates?: number;
  /** Dashboard embed uses a denser card treatment. */
  compact?: boolean;
  /** Full install surfaces import/paste actions; dashboard does not. */
  showSourceActions?: boolean;
};

// Template entry surface for the Dashboard and source picker. A Server
// Component: each affordance deep-links into the install page (which proceeds
// inline to live states), so it carries no client callbacks. Templates lead;
// `owner/repo` import + paste SKILL.md are the secondary source strip.
export function InstallEntry({
  templates,
  quickstart = false,
  maxTemplates,
  compact = false,
  showSourceActions = true,
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
      ) : null}

      {showSourceActions ? (
        <div className="flex flex-wrap gap-md border-t border-border pt-lg">
          <Button asChild variant="outline">
            <Link href="/fleets/new">Import from GitHub or paste SKILL.md</Link>
          </Button>
          {quickstart ? (
            <Button asChild variant="ghost" size="sm">
              <a href={QUICKSTART_URL} target="_blank" rel="noopener noreferrer">
                Quick start
              </a>
            </Button>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
