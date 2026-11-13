"use client";

import { Button, cn, Spinner } from "@agentsfleet/design-system";
import { CaretDownIcon, UserCircleIcon } from "@phosphor-icons/react/ssr";
import type { ComponentProps } from "react";
import { useCurrentUser } from "@/lib/auth/client";

type WorkspaceSwitcherTriggerProps = Omit<ComponentProps<typeof Button>, "children" | "size" | "variant"> & {
  activeLabel: string;
  busy?: boolean;
  failed?: boolean;
  collapsed?: boolean;
};

export function WorkspaceSwitcherTrigger({ activeLabel, busy = false, failed = false, collapsed = false, "aria-label": menuLabel = "Workspace and account menu", ...props }: WorkspaceSwitcherTriggerProps) {
  const { displayName, emailAddress } = useCurrentUser();
  const name = displayName || emailAddress?.split("@")[0];
  const initials = name?.trim().split(/\s+/).slice(0, 2).map((part) => part[0]).join("").toUpperCase();
  const displayLabel = name || "Your workspace";
  const subtitle = failed ? "Retry account menu" : activeLabel;
  const visibleLabel = [!busy && initials, !collapsed && displayLabel, !collapsed && subtitle].filter(Boolean).join(" ");
  const accessibleLabel = visibleLabel ? `${menuLabel}: ${visibleLabel}` : menuLabel;
  return <Button type="button" variant="ghost" size="lg" title={collapsed ? activeLabel : undefined} aria-label={accessibleLabel}
    className={cn("group h-11 min-h-11 w-full min-w-0 justify-center gap-2.5 px-0 py-sm hover:bg-muted hover:text-foreground md:h-12 md:min-h-12", !collapsed && "md:justify-start md:px-lg")}
    {...props}>
    <span className="flex size-8 shrink-0 items-center justify-center rounded-full bg-avatar text-avatar-foreground">
      {busy ? <Spinner size="sm" srLabel="Loading workspace menu" /> : initials
        ? <span aria-hidden="true" className="text-label font-extrabold">{initials}</span> : <UserCircleIcon size={18} aria-hidden="true" />}
    </span>{" "}
    {!collapsed ? <>
      <span className="hidden min-w-0 flex-1 flex-col items-start md:flex">
        <span className="w-full truncate text-left text-body-sm font-semibold leading-body-sm text-foreground" title={name}>{displayLabel}</span>{" "}
        <span className="w-full truncate text-left text-label font-normal leading-label text-muted-foreground" title={activeLabel}>{subtitle}</span>
      </span>
      <CaretDownIcon size={14} className="hidden shrink-0 text-muted-foreground md:block" aria-hidden="true" />
    </> : null}
  </Button>;
}
