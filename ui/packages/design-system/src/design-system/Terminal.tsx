"use client";

import { type ComponentProps, type ReactNode, useId, useState, useCallback } from "react";
import { cn } from "../utils";

type Props = Omit<ComponentProps<"div">, "children"> & {
  label?: string;
  green?: boolean;
  copyable?: boolean;
  children: ReactNode;
};

/*
 * Terminal — monospace code block with optional copy affordance.
 * Operational mono on the deepest surface; green variant for
 * "success"-flavored demos. Per spec, --pulse is reserved for live
 * signals — Terminal text uses --text (text-foreground), not --pulse.
 */
export default function Terminal({ label, green, copyable, children, className, ...rest }: Props) {
  const id = useId();
  const [copied, setCopied] = useState(false);

  const handleCopy = useCallback(() => {
    const text = typeof children === "string" ? children : "";
    navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  }, [children]);

  return (
    <div className={cn("relative", className)} {...rest}>
      <pre
        className={cn(
          "m-0 overflow-auto rounded-md border px-xl py-lg text-mono font-mono",
          "bg-background",
          green ? "border-success text-success" : "border-border text-foreground",
          copyable && "pr-[5.5rem]",
        )}
        aria-label={label}
        aria-describedby={label ? undefined : id}
        data-command={typeof children === "string" ? children : undefined}
      >
        <code>{children}</code>
        {!label && (
          <span id={id} className="sr-only">
            Code block
          </span>
        )}
      </pre>
      {copyable && (
        <button
          type="button"
          onClick={handleCopy}
          aria-label={copied ? "Copied!" : "Copy command"}
          data-testid="copy-btn"
          className={cn(
            "absolute top-[0.6rem] right-[0.6rem] cursor-pointer whitespace-nowrap",
            "rounded-sm border px-md py-1 font-mono text-label bg-secondary",
            "transition-colors ease-snap",
            copied
              ? "border-success text-success"
              : "border-border text-muted-foreground hover:border-border-strong hover:text-foreground",
          )}
        >
          {copied ? "✓ Copied" : "Copy"}
        </button>
      )}
    </div>
  );
}
