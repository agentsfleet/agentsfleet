"use client";

import type { CodeHeaderProps } from "@assistant-ui/react-markdown";
import { CopyButton } from "@agentsfleet/design-system";

/** Official assistant-ui Markdown CodeHeader slot, using shared controls.
 * Reference: https://r.assistant-ui.com/markdown-text.json */
export function CodeHeader({language, code}: CodeHeaderProps) {
  return <div className="flex items-center justify-between gap-md rounded-t-md border border-b-0 border-border bg-muted px-md py-xs text-label">
    <span className="font-sans text-muted-foreground">{language || "Code"}</span>
    <CopyButton value={code} label="Copy code" />
  </div>;
}
