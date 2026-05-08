"use client";

import { type ComponentProps, type ReactNode, useId, useState, useCallback } from "react";
import { cn } from "../utils";
import { Button } from "./Button";

type Props = Omit<ComponentProps<"div">, "children"> & {
  label?: string;
  green?: boolean;
  copyable?: boolean;
  children: ReactNode;
};

/*
 * Terminal — monospace code block rendered as a recognisable terminal
 * window: a chrome strip across the top (three muted-dot affordances
 * on the left, optional label centred in the chrome, copy button on
 * the right), then the mono body underneath. Per DESIGN_SYSTEM.md
 * "Operational Restraint": borders > shadows, no traffic-light
 * red/yellow/green decoration — the dots use muted/subtle/border
 * tokens so the chrome reads as terminal but never as macOS skin.
 *
 * --pulse is currency, reserved for live signals; Terminal text uses
 * --text (text-foreground) for the body. The `green` variant flips
 * the body border to success-green for "success"-flavoured demos.
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
    <div
      className={cn(
        // `bg-surface-deep` is one shade below the page --bg per
        // `.cli` in the canonical preview — terminal chrome reads
        // as "lower than the page itself". Mirrored across light
        // mode by the matching token in tokens.css.
        "overflow-hidden rounded-md border bg-surface-deep",
        green ? "border-success" : "border-border",
        className,
      )}
      {...rest}
    >
      {/* Chrome strip — the visual cue that this is a terminal window. */}
      <div
        className={cn(
          "flex items-center gap-2 px-md py-sm",
          "border-b bg-muted/30",
          green ? "border-success" : "border-border",
        )}
      >
        {/* Three restraint-mode dots. Muted tokens, not macOS traffic
         * lights. Read as "terminal window chrome" without the skin. */}
        <span className="flex items-center gap-1.5" aria-hidden="true">
          <span className="size-2.5 rounded-full bg-border-strong" />
          <span className="size-2.5 rounded-full bg-muted-foreground/50" />
          <span className="size-2.5 rounded-full bg-muted-foreground/30" />
        </span>
        {label && (
          <span className="flex-1 text-center font-mono text-label text-muted-foreground truncate">
            {label}
          </span>
        )}
        {copyable && (
          <Button
            type="button"
            variant={copied ? "outline" : "secondary"}
            size="sm"
            onClick={handleCopy}
            aria-label={copied ? "Copied!" : "Copy command"}
            data-testid="copy-btn"
            className={cn(
              "ml-auto h-auto py-0.5 text-label font-mono",
              copied && "border-success text-success",
            )}
          >
            {copied ? "✓ Copied" : "Copy"}
          </Button>
        )}
      </div>
      <pre
        className={cn(
          "m-0 overflow-auto px-xl py-lg text-mono font-mono",
          green ? "text-success" : "text-foreground",
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
    </div>
  );
}
