import type { ReactNode } from "react";
import { Badge, Card } from "@agentsfleet/design-system";
import type { FleetLibraryGalleryEntry } from "@/lib/types";

const NEEDS_PREFIX = "needs:";

type Props = {
  template: FleetLibraryGalleryEntry;
  compact?: boolean;
  // The call-to-action slot — a link on the dashboard gallery
  // (/fleets/new?template=<id>) or an in-page "Use template" button on the
  // install flow. Kept as a slot so this card stays presentational and shared
  // across the server (dashboard) and client (install) trees.
  action: ReactNode;
};

// Presentational template card: name, description, the credentials it needs,
// and a caller-supplied action.
export function TemplateCard({ template, compact = false, action }: Props) {
  return (
    <Card className={compact ? "flex flex-col gap-2 p-md" : "flex flex-col gap-3 p-lg"}>
      <div className={compact ? "space-y-0.5" : "space-y-1"}>
        <h3 className="font-medium text-foreground">{template.name}</h3>
        <p className="text-body-sm leading-body-sm text-muted-foreground">{template.description}</p>
      </div>
      {template.requirements.credentials.length > 0 ? (
        <div className="flex flex-wrap gap-1.5">
          {template.requirements.credentials.map((name) => (
            <Badge key={name} variant="amber">
              {NEEDS_PREFIX} {name}
            </Badge>
          ))}
        </div>
      ) : null}
      <div className={compact ? "mt-auto pt-sm" : "mt-auto pt-md"}>{action}</div>
    </Card>
  );
}
