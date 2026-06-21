import type { ReactNode } from "react";
import { Badge, Card } from "@agentsfleet/design-system";
import type { FleetTemplate } from "@/lib/types";

const NEEDS_PREFIX = "needs:";

type Props = {
  template: FleetTemplate;
  // The call-to-action slot — a link on the dashboard gallery
  // (/fleets/new?template=<id>) or an in-page "Use template" button on the
  // install flow. Kept as a slot so this card stays presentational and shared
  // across the server (dashboard) and client (install) trees.
  action: ReactNode;
};

// Presentational template card: name, description, the credentials it needs,
// and a caller-supplied action.
export function TemplateCard({ template, action }: Props) {
  return (
    <Card className="flex flex-col gap-3 p-4">
      <div className="space-y-1">
        <h3 className="font-mono text-sm text-foreground">{template.name}</h3>
        <p className="text-xs text-muted-foreground">{template.description}</p>
      </div>
      {template.required_credentials.length > 0 ? (
        <div className="flex flex-wrap gap-1.5">
          {template.required_credentials.map((name) => (
            <Badge key={name} variant="amber">
              {NEEDS_PREFIX} {name}
            </Badge>
          ))}
        </div>
      ) : null}
      <div className="mt-auto pt-1">{action}</div>
    </Card>
  );
}
