import type { ReactNode } from "react";
import {
  Badge,
  Card,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@agentsfleet/design-system";
import { Building2Icon, GlobeIcon } from "lucide-react";
import { VendorMark } from "@/components/domain/fleet-library/VendorMark";
import type { FleetLibraryGalleryEntry, FleetLibraryVisibility } from "@/lib/types";

// "Requires", not "needs": the badge states a prerequisite of the fleet, and
// this is the word the install flow and the docs use for the same fact.
const REQUIRES_PREFIX = "Requires";

// Every card is four rows tall — title, one line of description, one line of
// requirements, action — and none of them wraps.
//
// A card sits in an equal-height grid row, so the tallest card in a row sets
// the height of every card beside it. Unbounded, one verbose entry gave its
// five neighbours a screenful of dead space. Clamping the description to three
// lines fixed that and introduced a second problem: a description cut at "when
// the cause i…" states half a fact, and the rest was a click away in the
// install dialog — past the decision this card exists to inform.
//
// So the clamp tightens to one line and everything it hides answers on hover.
// A row of cards is now one height by construction rather than by luck, and
// nothing the card omits is unreachable from the card.
const DESCRIPTION_LINES = "line-clamp-1";

// One mark per credential, and the names on hover.
//
// Names as chips was the shape before, and three chips carrying words the
// length of `grafana` wrap to a second line — which makes that card taller
// than every neighbour in its grid row, the exact unevenness this card is
// being straightened to avoid. Marks are one line whatever a bundle needs.
//
// Metaphor glyphs were tried first and dropped. The integrations page
// decorates a provider with a lucide glyph and reads fine, but there the glyph
// sits BESIDE the provider's name — it decorates a label. Here it identifies
// alone, and a magnifying glass for Elastic is equally Algolia, Meilisearch,
// or just "search". So these are the providers' own marks, committed in
// `vendor-marks.ts`; an unrecognised provider draws a neutral key and is named
// in the tooltip like every other.
const MARK_CEILING = 5;

// The overflow chip, e.g. "+2". Its own constant because it is copy, and copy
// is a named constant here (RULE UFS).
const MORE_PREFIX = "+";

// Which catalogue an entry came from.
//
// Two entries can share a name across the two tiers — the platform catalogue
// and a workspace's own copy of the same bundle — and the gallery is the one
// place a person CHOOSES between them, so this cannot be dropped. It is a mark
// rather than a word to keep the title row on one line; the word itself stays
// in the tooltip and, for anyone not using a pointer, in the accessible name.
const TIER_LABEL: Record<FleetLibraryVisibility, string> = {
  platform: "Platform catalogue",
  tenant: "This workspace",
};

const TIER_ICON: Record<FleetLibraryVisibility, typeof GlobeIcon> = {
  platform: GlobeIcon,
  tenant: Building2Icon,
};

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
  const tier = TIER_LABEL[entry.visibility];
  const TierIcon = TIER_ICON[entry.visibility];

  return (
    // Keyed by catalog id so a test can assert an entry appears exactly once —
    // a duplicate row in the catalog is only ever visible here, in the gallery.
    // One scale down the card: the system's own inset, gap-lg between blocks,
    // gap-sm inside the title stack. It ran p-lg/gap-3/space-y-1/pt-md before,
    // which measured 15 · 12 · 16 · 19 · 15 down the card — five gaps, four
    // numbers, none of them the card inset above and below them.
    <Card data-testid={`library-card-${entry.id}`} className="flex flex-col gap-lg">
      <div className="flex flex-col gap-sm">
        <div className="flex items-center gap-sm">
          <h3 className="min-w-0 truncate font-medium text-foreground">{entry.name}</h3>
          <Tooltip>
            <TooltipTrigger asChild>
              {/* The mark carries the word as its accessible name, so the tier
               * is readable without a pointer — a tooltip alone would hide the
               * one signal that separates two identically named entries. */}
              <span
                data-testid={`library-card-tier-${entry.id}`}
                className="shrink-0 text-muted-foreground"
              >
                <TierIcon aria-hidden="true" className="size-4" />
                {/* Visually hidden rather than `aria-label`: a label on a
                 * generic element is not reliably announced, and this word is
                 * the only thing separating two identically named entries. */}
                <span className="sr-only">{tier}</span>
              </span>
            </TooltipTrigger>
            <TooltipContent>{tier}</TooltipContent>
          </Tooltip>
        </div>
        <Tooltip>
          <TooltipTrigger asChild>
            <p
              data-testid={`library-card-description-${entry.id}`}
              className={`text-body-sm leading-body-sm text-muted-foreground ${DESCRIPTION_LINES}`}
            >
              {entry.description}
            </p>
          </TooltipTrigger>
          <TooltipContent className="max-w-prose">{entry.description}</TooltipContent>
        </Tooltip>
      </div>
      {credentials.length > 0 ? (
        <Tooltip>
          <TooltipTrigger asChild>
            {/* `w-fit`: the row is the hover target, so it ends where the marks
             * do rather than spanning the card and arming a tooltip over empty
             * space. */}
            <div
              data-testid={`library-card-requires-${entry.id}`}
              className="flex w-fit items-center gap-sm text-muted-foreground"
            >
              {/* Muted, not amber: nothing is wrong here. Amber is this
               * system's warning colour, and a fleet naming the credential it
               * will ask for is a fact about the fleet, not a fault in the
               * workspace. */}
              {credentials.slice(0, MARK_CEILING).map((credential) => (
                <VendorMark key={credential} credential={credential} />
              ))}
              {credentials.length > MARK_CEILING ? (
                <Badge>
                  {MORE_PREFIX}
                  {credentials.length - MARK_CEILING}
                </Badge>
              ) : null}
            </div>
          </TooltipTrigger>
          {/* The names. No mark identifies a provider to a reader who does not
           * already know it, and the neutral glyph identifies nothing at all,
           * so the row never draws without this naming what it drew. */}
          <TooltipContent className="max-w-prose">
            {REQUIRES_PREFIX}: {credentials.join(", ")}
          </TooltipContent>
        </Tooltip>
      ) : null}
      <div className="mt-auto">{action}</div>
    </Card>
  );
}
