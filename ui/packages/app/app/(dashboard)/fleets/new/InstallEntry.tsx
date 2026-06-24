import Link from "next/link";
import { Button } from "@agentsfleet/design-system";
import type { FleetTemplate } from "@/lib/types";
import { TemplateCard } from "./TemplateCard";

const QUICKSTART_URL = "https://docs.agentsfleet.net/quickstart";

type Props = {
  templates: FleetTemplate[];
  // Optional trailing actions appended to the source strip (e.g. a Quick start
  // link on the dashboard card). The Fleets empty-state passes none.
  quickstart?: boolean;
};

// The single shared install entry surface, composed verbatim by the Dashboard
// "Start your fleet" card and the Fleets empty-state — one source, no
// hand-rolled duplicates. A Server Component: each affordance deep-links into
// the install page (which proceeds inline to the live states), so it carries no
// client callbacks. Templates lead; `owner/repo` import + paste SKILL.md are the
// secondary source strip.
export function InstallEntry({ templates, quickstart = false }: Props) {
  return (
    <div className="space-y-5">
      {templates.length > 0 ? (
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          {templates.map((template) => (
            <TemplateCard
              key={template.id}
              template={template}
              action={
                <Button asChild size="sm">
                  <Link href={`/fleets/new?template=${template.id}`}>Use template</Link>
                </Button>
              }
            />
          ))}
        </div>
      ) : null}

      <div className="flex flex-wrap gap-3 border-t border-border pt-5">
        <Button asChild variant="ghost" size="sm">
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
    </div>
  );
}
