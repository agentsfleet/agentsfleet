import { KeyRoundIcon } from "lucide-react";
import { vendorMark } from "./vendor-marks";

// One credential, drawn.
//
// A recognised provider gets its own mark; anything else gets a neutral key.
// The fallback is not a rare path — Simple Icons ships no Slack mark, and Slack
// is in this product's own fixtures — so it has to read as a deliberate "a
// credential, unnamed here" rather than as a missing image.
//
// Nothing here renders the provider's NAME. A mark identifies only what a
// reader already knows, and the neutral glyph identifies nothing at all, so
// every caller is responsible for naming the set it draws. `LibraryCard` does
// that in the tooltip over the whole row.

/** The square a mark occupies, in the design system's icon scale. */
const MARK_SIZE = "size-4";

/** Simple Icons draws every mark on this box. */
const MARK_VIEWBOX = "0 0 24 24";

export type VendorMarkProps = {
  /** The credential as the bundle spells it, e.g. `github`. */
  readonly credential: string;
};

export function VendorMark({ credential }: VendorMarkProps) {
  const mark = vendorMark(credential);

  if (!mark) {
    // `aria-hidden`: the row's tooltip and its accessible name carry every
    // credential by name, so announcing an unnamed glyph would only repeat
    // "image" into a screen reader between the words that mean something.
    return <KeyRoundIcon aria-hidden="true" className={MARK_SIZE} />;
  }

  return (
    <svg
      aria-hidden="true"
      viewBox={MARK_VIEWBOX}
      className={MARK_SIZE}
      fill="currentColor"
      data-vendor-mark={credential}
    >
      {/* The mark carries no <title>: it is decorative here because the
       * credential's name is already in the row's tooltip. A title would put
       * the word in the accessibility tree twice. */}
      <path d={mark.path} />
    </svg>
  );
}
