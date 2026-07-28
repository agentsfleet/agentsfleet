import type { ReactNode } from "react";
import { Badge, Card } from "@agentsfleet/design-system";
import type { FleetLibraryGalleryEntry } from "@/lib/types";

const NEEDS_PREFIX = "needs:";

type Props = {
  entry: FleetLibraryGalleryEntry;
  // The call-to-action slot — the install picker's "Use entry" button. Kept
  // as a slot so this card stays presentational.
  action: ReactNode;
};

// Presentational library-entry card: name, description, the credentials it
// needs, and a caller-supplied action. The `compact` variant left with the
// dashboard surface that used it — the install picker is the one consumer now.
export function LibraryCard({ entry, action }: Props) {
  return (
    // Keyed by catalog id so a test can assert an entry appears exactly once —
    // a duplicate row in the catalog is only ever visible here, in the gallery.
    <Card data-testid={`library-card-${entry.id}`} className="flex flex-col gap-3 p-lg">
      <div className="space-y-1">
        <h3 className="font-medium text-foreground">{entry.name}</h3>
        <p className="text-body-sm leading-body-sm text-muted-foreground">{entry.description}</p>
      </div>
      {entry.requirements.credentials.length > 0 ? (
        <div className="flex flex-wrap gap-1.5">
          {entry.requirements.credentials.map((name) => (
            <Badge key={name} variant="amber">
              {NEEDS_PREFIX} {name}
            </Badge>
          ))}
        </div>
      ) : null}
      <div className="mt-auto pt-md">{action}</div>
    </Card>
  );
}
