"use client";

import { useState } from "react";
import { CheckIcon, CopyIcon } from "lucide-react";
import { Button } from "./Button";
import { cn } from "../utils";
import { useResettableTimeout } from "./use-resettable-timeout";

/*
 * CopyButton — icon-only clipboard affordance for values users paste
 * elsewhere (workspace IDs, key IDs, names). Shows a check for a moment
 * after copying; the accessible name flips with it so screen readers
 * announce the result. Client-only (navigator.clipboard).
 */

const COPIED_RESET_MS = 2_000;
const COPIED_LABEL = "Copied";

export interface CopyButtonProps {
  /** Text written to the clipboard. */
  value: string;
  /** Accessible name, e.g. "Copy workspace ID". */
  label: string;
  className?: string;
}

export function CopyButton({ value, label, className }: CopyButtonProps) {
  const [copied, setCopied] = useState(false);
  const reset = useResettableTimeout();

  async function copy() {
    try {
      await navigator.clipboard.writeText(value);
    } catch {
      return; // clipboard unavailable (permissions / insecure context)
    }
    setCopied(true);
    reset.start(() => setCopied(false), COPIED_RESET_MS);
  }

  return (
    <Button
      type="button"
      variant="ghost"
      size="icon-sm"
      onClick={() => {
        void copy();
      }}
      aria-label={copied ? COPIED_LABEL : label}
      title={copied ? COPIED_LABEL : label}
      // icon-sm keeps its 24px visuals; the ::after overlay widens the
      // interactive area (40px, 48px on touch) toward the 44px floor.
      className={cn(
        "relative after:absolute after:-inset-md pointer-coarse:after:-inset-lg",
        className,
      )}
      data-slot="copy-button"
    >
      {copied ? (
        <CheckIcon size={14} className="text-success" aria-hidden="true" />
      ) : (
        <CopyIcon size={14} aria-hidden="true" />
      )}
    </Button>
  );
}

export default CopyButton;
