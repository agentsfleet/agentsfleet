"use client";

import { useState } from "react";
import { CheckIcon, CopyIcon, XIcon } from "lucide-react";
import { Button } from "./Button";
import { cn } from "../utils";
import { useResettableTimeout } from "./use-resettable-timeout";

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

type CopyOutcome = "idle" | "copied" | "failed";

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
  onOutcomeChange?: (outcome: "idle" | "copied" | "failed") => void;
  className?: string;
}

export function CopyButton({ value, label, showLabel = false, onOutcomeChange, className }: CopyButtonProps) {
  const [outcome, setOutcome] = useState<CopyOutcome>("idle");
  const reset = useResettableTimeout();

  async function copy() {
    let next: CopyOutcome;
    try {
      await navigator.clipboard.writeText(value);
      next = "copied";
    } catch {
      // Clipboard unavailable (permissions / insecure context). Say so.
      next = "failed";
    }
    setOutcome(next);
    onOutcomeChange?.(next);
    reset.start(() => {
      setOutcome("idle");
      onOutcomeChange?.("idle");
    }, COPY_RESET_MS);
  }

  const accessibleName =
    outcome === "copied" ? COPIED_LABEL : outcome === "failed" ? FAILED_LABEL : label;

  const icon =
    outcome === "copied" ? (
      <CheckIcon size={14} className="text-success" aria-hidden="true" />
    ) : outcome === "failed" ? (
      <XIcon size={14} className="text-destructive" aria-hidden="true" />
    ) : (
      <CopyIcon size={14} aria-hidden="true" />
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
      // interactive area (40px, 48px on touch) toward the 44px floor. A labelled
      // button already clears the floor on its own.
      className={cn(
        !showLabel && "relative after:absolute after:-inset-md pointer-coarse:after:-inset-lg",
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
        <span role="status" aria-live="polite">
          {accessibleName}
        </span>
      ) : (
        <span className="sr-only" role="status" aria-live="polite">
          {outcome === "idle" ? "" : accessibleName}
        </span>
      )}
    </Button>
  );
}

export default CopyButton;
