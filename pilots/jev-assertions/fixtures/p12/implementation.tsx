"use client";

import { useState } from "react";
import { CheckIcon, CopyIcon, XIcon } from "lucide-react";
import { Button } from "../../helpers/Button";
import { cn } from "../../helpers/utils";
import { useResettableTimeout } from "../../helpers/use-resettable-timeout";

/*
 * CopyButton — icon-only clipboard affordance for values users paste
 * elsewhere (workspace IDs, key IDs, names). Shows a check for a moment
 * after copying; the accessible name flips with it so screen readers
 * announce the result. Client-only (navigator.clipboard).
 *
 * A failed write is REPORTED, never swallowed. Some of the values that pass
 * through here — a one-time API key, a runner enrollment token — are shown
 * exactly once and cannot be recovered. A copy that silently did nothing, on a
 * button that looks like it worked, costs the user that value permanently. The
 * clipboard is genuinely unavailable often enough (insecure context, denied
 * permission, no user gesture) that this is not a theoretical branch.
 */

/** How long the copied/failed outcome shows before reverting. Exported so a test
 *  pins the real window instead of re-spelling the number (RULE UFS). */
export const COPY_RESET_MS = 2_000;
const COPIED_LABEL = "Copied";
const FAILED_LABEL = "Copy failed — select the value and copy it manually";

const OUTCOME_IDLE = "idle";
const OUTCOME_COPIED = "copied";
const OUTCOME_FAILED = "failed";
const ARIA_HIDDEN_TRUE = "true";
const STATUS_ROLE = "status";
const LIVE_POLITENESS = "polite";

type CopyOutcome = typeof OUTCOME_IDLE | typeof OUTCOME_COPIED | typeof OUTCOME_FAILED;

export interface CopyButtonProps {
  /** Text written to the clipboard. */
  value: string;
  /** Accessible name, e.g. "Copy workspace ID". */
  label: string;
  /**
   * Render `label` as visible text beside the icon. Icon-only (the default) is
   * right beside a value the user can already see — a table cell, a field. Set
   * this where copying IS the page's action and an icon alone would be a hunt:
   * the CLI verification code, a one-time secret's reveal panel.
   */
  showLabel?: boolean;
  /**
   * Observe outcome transitions. For one-time secrets the 2s failed flash is
   * not enough — the dialog wants a PERSISTENT "copy it manually" line once a
   * write has failed, and this is how it knows without a second clipboard path.
   */
  onOutcomeChange?: (outcome: CopyOutcome) => void;
  className?: string;
}

export function CopyButton({ value, label, showLabel = false, onOutcomeChange, className }: CopyButtonProps) {
  const [outcome, setOutcome] = useState<CopyOutcome>(OUTCOME_IDLE);
  const reset = useResettableTimeout();

  async function copy() {
    let next: CopyOutcome;
    try {
      await navigator.clipboard.writeText(value);
      next = OUTCOME_COPIED;
    } catch {
      // Clipboard unavailable (permissions / insecure context). Say so.
      next = OUTCOME_FAILED;
    }
    setOutcome(next);
    onOutcomeChange?.(next);
    reset.start(() => {
      setOutcome(OUTCOME_IDLE);
      onOutcomeChange?.(OUTCOME_IDLE);
    }, COPY_RESET_MS);
  }

  const accessibleName =
    outcome === OUTCOME_COPIED ? COPIED_LABEL : outcome === OUTCOME_FAILED ? FAILED_LABEL : label;

  const icon =
    outcome === OUTCOME_COPIED ? (
      <CheckIcon size={14} className="text-success" aria-hidden={ARIA_HIDDEN_TRUE} />
    ) : outcome === OUTCOME_FAILED ? (
      <XIcon size={14} className="text-destructive" aria-hidden={ARIA_HIDDEN_TRUE} />
    ) : (
      <CopyIcon size={14} aria-hidden={ARIA_HIDDEN_TRUE} />
    );

  return (
    <Button
      type="button"
      variant={showLabel ? "secondary" : "ghost"}
      size={showLabel ? "sm" : "icon-sm"}
      onClick={() => {
        void copy();
      }}
      aria-label={accessibleName}
      title={accessibleName}
      // Icon-only keeps its 24px visuals; the ::after overlay widens the
      // interactive area vertically, where the row's own padding leaves room.
      // A labelled button already clears the floor on its own.
      //
      // The expansion is deliberately vertical ONLY. It used to be `-inset-md`
      // on all four sides, and the arithmetic did not close: `--sp-md` is 8px,
      // so 24 + 16 = 40px of hit area sat on a 28px pitch (24px button, 4px
      // `gap-1`), overlapping a neighbour by 12px. `elementFromPoint` put the
      // first 3px of the visible Rename icon inside Copy's hit area, and on
      // `/admin/fleet-libraries` the stolen neighbour is destructive.
      //
      // Horizontal reach is not needed anyway: `Button`'s base carries
      // `pointer-coarse:min-w-11`, so on the pointer where 44px is required the
      // button box already supplies it. Adding overlay width on top of that
      // only reached into the next control.
      className={cn(
        !showLabel &&
          "relative after:absolute after:-inset-y-md after:inset-x-0 pointer-coarse:after:-inset-y-lg",
        className,
      )}
      data-slot="copy-button"
      data-outcome={outcome}
    >
      {icon}
      {/* One node carries the outcome, never two — a duplicated string inside one
          button is both a DOM smell and unassertable. Labelled: the visible text IS
          the live region. Icon-only: there is no visible text, so an off-screen one
          does the announcing, because an icon swap alone is not announced and the
          failure is the branch a user must not miss. */}
      {showLabel ? (
        <span role={STATUS_ROLE} aria-live={LIVE_POLITENESS}>
          {accessibleName}
        </span>
      ) : (
        <span className="sr-only" role={STATUS_ROLE} aria-live={LIVE_POLITENESS}>
          {outcome === OUTCOME_IDLE ? "" : accessibleName}
        </span>
      )}
    </Button>
  );
}

export default CopyButton;
