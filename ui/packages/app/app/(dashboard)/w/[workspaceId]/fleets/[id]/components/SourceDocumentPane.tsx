"use client";

import { useLayoutEffect, useRef } from "react";
import { CopyButton, Textarea, cn } from "@agentsfleet/design-system";

const SERVER_SOURCE_LABEL = "Current server version";
const UNSAVED_DRAFT_LABEL = "Your unsaved draft";
const STALE_SOURCE_REVIEW_LABEL = "Review server changes";

export function StaleSourceComparison({ base, draft }: { base: string; draft: string }) {
  return (
    <details open className="rounded-md border border-warning/40 bg-warning/5 px-3 py-2">
      <summary className="cursor-pointer font-sans text-xs font-medium text-foreground">
        {STALE_SOURCE_REVIEW_LABEL}
      </summary>
      <div className="grid gap-sm pt-sm md:grid-cols-2">
        <SourceComparisonPane label={SERVER_SOURCE_LABEL} value={base} />
        <SourceComparisonPane label={UNSAVED_DRAFT_LABEL} value={draft} />
      </div>
    </details>
  );
}

function SourceComparisonPane({ label, value }: { label: string; value: string }) {
  return (
    <div data-testid={`source-comparison-${label.toLowerCase().replaceAll(" ", "-")}`} className="min-w-0">
      <p className="mb-xs font-sans text-xs text-muted-foreground">{label}</p>
      <Textarea readOnly value={value} aria-label={label} rows={8} className="max-h-48 overflow-auto rounded-sm border border-border bg-muted/30 px-2 py-1 font-mono text-xs leading-mono text-foreground" />
    </div>
  );
}

export function DocumentPane({
  label,
  editing,
  value,
  emptyHint,
  onChange,
  fillAvailableSpace,
}: {
  label: string;
  editing: boolean;
  value: string;
  emptyHint: string;
  onChange: (value: string) => void;
  fillAvailableSpace: boolean;
}) {
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  useLayoutEffect(() => {
    const textarea = textareaRef.current;
    if (!editing || !textarea) return;

    textarea.focus();
    focusEditorAtStart(textarea);
  }, [editing]);

  if (editing) {
    return (
      <Textarea
        aria-label={`Edit ${label}`}
        ref={textareaRef}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        className={cn(
          "min-h-64 w-full resize-y font-mono text-xs leading-mono",
          fillAvailableSpace && "min-h-96 flex-1",
        )}
      />
    );
  }
  if (value.length === 0) return <p className="text-sm text-muted-foreground">{emptyHint}</p>;
  return <SourcePreview label={label} value={value} fillAvailableSpace={fillAvailableSpace} />;
}

function SourcePreview({ label, value, fillAvailableSpace }: {
  label: string;
  value: string;
  fillAvailableSpace: boolean;
}) {
  return (
    <div className={cn("relative", fillAvailableSpace && "flex min-h-0 flex-1 flex-col")}>
      <div className="absolute right-xs top-xs">
        <CopyButton value={value} label={`Copy ${label}`} />
      </div>
      <Textarea
        readOnly
        value={value}
        rows={16}
        aria-label={label}
        className={cn(
          "max-h-96 overflow-auto rounded-md border border-border bg-muted/30 px-3 py-2 font-mono text-xs leading-mono text-foreground",
          fillAvailableSpace && "max-h-none min-h-96 flex-1",
        )}
      />
    </div>
  );
}

function focusEditorAtStart(textarea: HTMLTextAreaElement) {
  textarea.setSelectionRange(0, 0);
  textarea.scrollTop = 0;
}
