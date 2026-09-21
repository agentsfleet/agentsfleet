import type { ReactNode } from "react";
import { Badge, Card } from "@agentsfleet/design-system";
import type { FleetLibraryGalleryEntry } from "@/lib/types";

// "Requires", not "needs": the badge states a prerequisite of the fleet, and
// this is the word the install flow and the docs use for the same fact.
const REQUIRES_PREFIX = "requires:";

// How many description lines and credential chips a card shows before it
// stops.
//
// A card sits in an equal-height grid row, so the tallest card in a row sets
// the height of every card beside it. Unbounded, one verbose entry gives its
// five neighbours a screenful of dead space — which is exactly what a seeded
// catalogue entry with a five-sentence description and five credentials did.
// Bounding both means a row's height stops tracking its worst member.
//
// Three and three because that is what the other entries already occupy: the
// clamp is the shape the catalogue mostly has, enforced, rather than a new
// ceiling imposed on it. The full description is one click away in the install
// dialog, which is where someone deciding actually reads it.
const DESCRIPTION_LINES = "line-clamp-3";
const VISIBLE_CREDENTIALS = 3;

// The overflow chip, e.g. "+2". Its own constant because it is copy, and copy
// is a named constant here (RULE UFS).
const MORE_PREFIX = "+";

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
  const credentials = entry.requirements.credentials;
  const shown = credentials.slice(0, VISIBLE_CREDENTIALS);
  const hidden = credentials.length - shown.length;

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
        <p className={`text-body-sm leading-body-sm text-muted-foreground ${DESCRIPTION_LINES}`}>
          {entry.description}
        </p>
      </div>
      {credentials.length > 0 ? (
        <div className="flex flex-wrap gap-sm">
          {shown.map((name) => (
            // Muted, not amber: nothing is wrong here. Amber is this system's
            // warning colour, and a fleet naming the credential it will ask
            // for is a fact about the fleet, not a fault in the workspace.
            <Badge key={name}>
              {REQUIRES_PREFIX} {name}
            </Badge>
          ))}
          {hidden > 0 ? (
            // The count, not the names. A card answers "roughly what does this
            // need"; the install dialog answers "exactly what", and it is the
            // screen that can refuse for a missing one.
            <Badge title={credentials.slice(VISIBLE_CREDENTIALS).join(", ")}>
              {MORE_PREFIX}
              {hidden}
            </Badge>
          ) : null}
        </div>
      ) : null}
      <div className="mt-auto">{action}</div>
    </Card>
  );
}
