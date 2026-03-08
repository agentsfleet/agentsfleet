import { type ReactNode, useId, useState, useCallback } from "react";

type Props = {
  label?: string;
  green?: boolean;
  copyable?: boolean;
  children: ReactNode;
};

export default function Terminal({ label, green, copyable, children }: Props) {
  const id = useId();
  const [copied, setCopied] = useState(false);

  const handleCopy = useCallback(() => {
    const text = typeof children === "string" ? children : "";
    navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  }, [children]);

  const cls = ["z-terminal", green && "z-terminal--green", copyable && "z-terminal--copyable"]
    .filter(Boolean)
    .join(" ");

  return (
    <div className="z-terminal-wrap">
      <pre
        className={cls}
        aria-label={label}
        aria-describedby={label ? undefined : id}
        data-command={typeof children === "string" ? children : undefined}
      >
        <code>{children}</code>
        {!label && <span id={id} className="sr-only">Code block</span>}
      </pre>
      {copyable && (
        <button
          type="button"
          className="z-copy-btn"
          onClick={handleCopy}
          aria-label={copied ? "Copied!" : "Copy command"}
          data-testid="copy-btn"
        >
          {copied ? "✓ Copied" : "Copy"}
        </button>
      )}
    </div>
  );
}
