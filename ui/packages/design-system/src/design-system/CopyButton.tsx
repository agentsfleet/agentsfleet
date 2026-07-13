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

const RESET_MS = 2_000;
const COPIED_LABEL = "Copied";
const FAILED_LABEL = "Copy failed — select the value and copy it manually";

type CopyOutcome = "idle" | "copied" | "failed";

export interface CopyButtonProps {
  /** Text written to the clipboard. */
  value: string;
  /** Accessible name, e.g. "Copy workspace ID". */
  label: string;
  className?: string;
}

export function CopyButton({ value, label, className }: CopyButtonProps) {
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
    reset.start(() => setOutcome("idle"), RESET_MS);
  }

  const accessibleName =
    outcome === "copied" ? COPIED_LABEL : outcome === "failed" ? FAILED_LABEL : label;

  return (
    <Button
      type="button"
      variant="ghost"
      size="icon-sm"
      onClick={() => {
        void copy();
      }}
      aria-label={accessibleName}
      title={accessibleName}
      // icon-sm keeps its 24px visuals; the ::after overlay widens the
      // interactive area (40px, 48px on touch) toward the 44px floor.
      className={cn(
        "relative after:absolute after:-inset-md pointer-coarse:after:-inset-lg",
        className,
      )}
      data-slot="copy-button"
      data-outcome={outcome}
    >
      {/* The live region is the failure's real carrier: an icon swap alone is
          not announced, and this is the branch a user must not miss. */}
      <span className="sr-only" role="status" aria-live="polite">
        {outcome === "idle" ? "" : accessibleName}
      </span>
      {outcome === "copied" ? (
        <CheckIcon size={14} className="text-success" aria-hidden="true" />
      ) : outcome === "failed" ? (
        <XIcon size={14} className="text-destructive" aria-hidden="true" />
      ) : (
        <CopyIcon size={14} aria-hidden="true" />
      )}
    </Button>
  );
}

export default CopyButton;
