"use client";

import { Button, Spinner } from "@agentsfleet/design-system";
import { ChevronDownIcon, FolderIcon } from "lucide-react";
import type { ComponentProps } from "react";

type WorkspaceSwitcherTriggerProps = Omit<
  ComponentProps<typeof Button>,
  "children" | "size" | "variant"
> & {
  activeLabel: string;
  busy?: boolean;
  failed?: boolean;
};

export function WorkspaceSwitcherTrigger({
  activeLabel,
  busy = false,
  failed = false,
  ...props
}: WorkspaceSwitcherTriggerProps) {
  return (
    <Button
      type="button"
      variant="outline"
      size="sm"
      className="bg-card font-mono text-eyebrow"
      {...props}
    >
      {busy ? (
        <Spinner size="sm" srLabel="Loading workspace menu" />
      ) : (
        <FolderIcon
          size={14}
          strokeWidth={1.75}
          aria-hidden="true"
          className="text-muted-foreground"
        />
      )}
      <span className="max-w-trim overflow-hidden text-ellipsis whitespace-nowrap">
        {failed ? "Retry workspace menu" : activeLabel}
      </span>
      <ChevronDownIcon size={14} aria-hidden="true" />
    </Button>
  );
}
