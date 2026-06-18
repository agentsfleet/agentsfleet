"use client";

import { useCallback, useState } from "react";
import { Button } from "@agentsfleet/design-system";

// Presentational chrome for the CLI-auth approve page. Extracted from
// page.tsx to keep that file under the length cap; these are internal to
// the route (the approve-flow logic stays in page.tsx).

const TOKEN_NAME_MAX_LEN = 64;
const COPY_RESET_MS = 2000;

export function PageShell({ children }: { children: React.ReactNode }) {
  return (
    <main className="min-h-screen flex items-center justify-center bg-background p-6">
      <div className="w-full max-w-md">{children}</div>
    </main>
  );
}

export function VerificationCodeDisplay({ code }: { code: string }) {
  return (
    <output
      aria-label="Verification code"
      className="block font-mono text-3xl tracking-widest text-center py-4"
    >
      {code}
    </output>
  );
}

export function CopyButton({ value }: { value: string }) {
  const [copied, setCopied] = useState(false);
  const onCopy = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(value);
      setCopied(true);
      window.setTimeout(() => setCopied(false), COPY_RESET_MS);
    } catch {
      setCopied(false);
    }
  }, [value]);
  return (
    <Button variant="secondary" onClick={() => void onCopy()}>
      {copied ? "Copied" : "Copy code"}
    </Button>
  );
}

export function sanitizeTokenName(raw: string): string {
  const trimmed = raw.slice(0, TOKEN_NAME_MAX_LEN);
  const printable = trimmed.replace(/[\x00-\x1f\x7f]/g, "");
  return printable.length > 0 ? printable : "your terminal";
}
