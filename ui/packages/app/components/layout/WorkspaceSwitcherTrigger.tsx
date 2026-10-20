"use client";

import { Button, cn, Spinner } from "@agentsfleet/design-system";
import { ChevronDownIcon, UserRoundIcon } from "lucide-react";
import type { ComponentProps } from "react";
import { useCurrentUser } from "@/lib/auth/client";

type WorkspaceSwitcherTriggerProps = Omit<ComponentProps<typeof Button>, "children" | "size" | "variant"> & {
  activeLabel: string;
  busy?: boolean;
  failed?: boolean;
  collapsed?: boolean;
};

export function WorkspaceSwitcherTrigger({ activeLabel, busy = false, failed = false, collapsed = false, ...props }: WorkspaceSwitcherTriggerProps) {
  const { displayName, emailAddress } = useCurrentUser();
  const name = displayName || emailAddress?.split("@")[0];
  const initials = name?.trim().split(/\s+/).slice(0, 2).map((part) => part[0]).join("").toUpperCase();
  return <Button type="button" variant="ghost" size="lg" title={activeLabel}
    className={cn("h-auto min-h-12 w-full min-w-0 gap-md py-sm", collapsed ? "justify-center px-0" : "justify-start px-md")}
    {...props}>
    <span className="flex size-8 shrink-0 items-center justify-center rounded-full bg-pulse/15 text-pulse">
      {busy ? <Spinner size="sm" srLabel="Loading workspace menu" /> : initials
        ? <span aria-hidden="true" className="text-label font-medium">{initials}</span> : <UserRoundIcon size={18} aria-hidden="true" />}
    </span>
    {!collapsed ? <>
      <span className="flex min-w-0 flex-1 flex-col items-start">
        <span className="w-full truncate text-left text-body-sm font-medium leading-body-sm" title={name}>{name || "Your workspace"}</span>
        <span className="w-full truncate text-left text-label font-normal leading-label text-muted-foreground" title={activeLabel}>{failed ? "Retry account menu" : activeLabel}</span>
      </span>
      <ChevronDownIcon size={14} className="shrink-0 text-muted-foreground" aria-hidden="true" />
    </> : null}
  </Button>;
}
