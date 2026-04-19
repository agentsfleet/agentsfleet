import { type ReactNode, useId, useState, useCallback } from "react";
import { cn } from "../utils";

type Props = {
  label?: string;
  green?: boolean;
  copyable?: boolean;
  children: ReactNode;
  className?: string;
};

/*
 * Terminal — monospace code block with optional copy affordance.
 * Cyan text by default; green variant for "success"-flavored demos.
 */
export default function Terminal({ label, green, copyable, children, className }: Props) {
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
    <div className={cn("relative", className)}>
      <pre
        className={cn(
          "m-0 overflow-auto rounded-md border p-[0.95rem_1.1rem] text-[0.88rem]",
          "bg-[var(--z-surface-terminal)]",
          green
            ? "border-[var(--z-glow-green)] text-success"
            : "border-border text-info",
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
            "rounded-sm border px-[0.7rem] py-1 font-mono text-xs bg-secondary",
            "transition-[color,border-color] duration-150",
            copied
              ? "border-success text-success"
              : "border-border text-muted-foreground hover:border-primary hover:text-foreground",
          )}
        >
          {copied ? "✓ Copied" : "Copy"}
        </button>
      )}
    </div>
  );
}
