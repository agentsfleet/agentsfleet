"use client";

import {
  type ComponentProps,
  type ReactNode,
  useEffect,
  useId,
  useRef,
  useState,
  useCallback,
} from "react";
import { cn } from "../utils";
import { Button } from "./Button";

type Props = Omit<ComponentProps<"div">, "children"> & {
  label?: string;
  green?: boolean;
  copyable?: boolean;
  /**
   * Plain-text payload for the clipboard. Required when `copyable` is
   * true and `children` is JSX (e.g. severity-coloured `<LogLine>`
   * blocks). When `children` is a string and `copyText` is unset, the
   * string itself is copied. When `copyable=true` but neither a string
   * child nor `copyText` is provided, the copy button is hidden — a
   * silent no-op affordance is worse than no affordance.
   */
  copyText?: string;
  /**
   * Opt-in install-demo animation: reveals child lines in sequence via CSS
   * (`[data-terminal-reveal]` in tokens.css), so the demo reads as "running
   * live". Reduced-motion shows every line at once. Purely presentational —
   * content is always in the DOM, never JS-gated. The per-line stagger in
   * tokens.css is tuned for up to ~6 lines; beyond that, extra lines still
   * reveal but without the staggered delay.
   */
  animate?: boolean;
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
export default function Terminal({ label, green, copyable, copyText, animate, children, className, ...rest }: Props) {
  const id = useId();
  const [copied, setCopied] = useState(false);
  const resetTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Clear any in-flight reset timer if the component unmounts mid-flash —
  // otherwise a Copy click followed by a fast unmount fires `setCopied`
  // on a dead component (React warning + memory hold on the closure).
  useEffect(
    () => () => {
      if (resetTimerRef.current) clearTimeout(resetTimerRef.current);
    },
    [],
  );

  // Resolve the clipboard payload: explicit `copyText` wins, otherwise
  // fall back to the children iff they're already a flat string. The
  // copy button only renders when one of these resolves to a non-empty
  // string (see render below) — no silent no-op affordances.
  const resolvedCopyText =
    typeof copyText === "string" && copyText.length > 0
      ? copyText
      : typeof children === "string"
        ? children
        : "";

  const handleCopy = useCallback(() => {
    // Clipboard access can reject (denied permission, non-secure context,
    // sandboxed iframe). Catch the rejection so the user-facing failure
    // path is silent rather than an unhandled promise rejection in the
    // console — the absence of the "Copied" flash is the visible signal.
    navigator.clipboard
      .writeText(resolvedCopyText)
      .then(() => {
        setCopied(true);
        if (resetTimerRef.current) clearTimeout(resetTimerRef.current);
        resetTimerRef.current = setTimeout(() => setCopied(false), 2000);
      })
      .catch(() => {
        /* clipboard denied — leave button in resting state */
      });
  }, [resolvedCopyText]);

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
        {copyable && resolvedCopyText.length > 0 && (
          // Copy button renders only when we have a non-empty string
          // to put on the clipboard — either the children themselves
          // (string mode) or the explicit `copyText` override
          // (JSX-children mode, e.g. severity-coloured <LogLine>
          // blocks). When neither is present, hide the affordance —
          // a silent no-op button is worse than no button.
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
        {/* `data-terminal-reveal` drives the staggered line-in animation in
          * tokens.css when `animate` is set; absent otherwise (static). */}
        <code data-terminal-reveal={animate ? "" : undefined}>{children}</code>
        {!label && (
          <span id={id} className="sr-only">
            Code block
          </span>
        )}
      </pre>
    </div>
  );
}
