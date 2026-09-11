import type { ReactNode } from "react";
import { Badge, Card } from "@agentsfleet/design-system";
import type { FleetLibraryGalleryEntry } from "@/lib/types";

// "Requires", not "needs": the badge states a prerequisite of the fleet, and
// this is the word the install flow and the docs use for the same fact.
const REQUIRES_PREFIX = "requires:";

type Props = {
  entry: FleetLibraryGalleryEntry;
  // The call-to-action slot — the install picker's "Install" button. Kept
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
    // One scale down the card: the system's own inset, gap-lg between blocks,
    // gap-sm inside the title stack. It ran p-lg/gap-3/space-y-1/pt-md before,
    // which measured 15 · 12 · 16 · 19 · 15 down the card — five gaps, four
    // numbers, none of them the card inset above and below them.
    <Card data-testid={`library-card-${entry.id}`} className="flex flex-col gap-lg">
      <div className="flex flex-col gap-sm">
        <h3 className="font-medium text-foreground">{entry.name}</h3>
        <p className="text-body-sm leading-body-sm text-muted-foreground">{entry.description}</p>
      </div>
      {entry.requirements.credentials.length > 0 ? (
        <div className="flex flex-wrap gap-sm">
          {entry.requirements.credentials.map((name) => (
            // Muted, not amber: nothing is wrong here. Amber is this system's
            // warning colour, and a fleet naming the credential it will ask
            // for is a fact about the fleet, not a fault in the workspace.
            <Badge key={name}>
              {REQUIRES_PREFIX} {name}
            </Badge>
          ))}
        </div>
      ) : null}
      <div className="mt-auto">{action}</div>
    </Card>
  );
}
