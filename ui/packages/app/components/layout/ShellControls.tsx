"use client";

import { useEffect, useRef, useState } from "react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import {
  MenuIcon,
  PanelLeftCloseIcon,
  PanelLeftOpenIcon,
  RefreshCwIcon,
} from "lucide-react";
import { Button, Spinner, WakePulse } from "@agentsfleet/design-system";
import { setAnalyticsContext } from "@/lib/analytics/posthog";
import type { TenantWorkspace } from "@/lib/api/workspaces";
import {
  DEFAULT_WORKSPACE_SUBPATH,
  workspaceIdFromPath,
  workspacePath,
} from "@/lib/workspace-routes";
import {
  createIntentModuleLoader,
  INTENT_MODULE_STATUS,
  maySpeculateOnHover,
  useIntentModule,
} from "@/components/domain/island-dynamic/intent-module-loader";
import {
  shellSidebarState,
  useShellSidebarCollapsed,
} from "./shell-sidebar-state";

const mobileNavigationLoader = createIntentModuleLoader(
  () => import("./MobileNavigationDialog"),
);

type ShellControlsProps = {
  workspaces: TenantWorkspace[];
  operatorScopes: string[];
  sidebarNavId: string;
};

export function ShellControls({
  workspaces,
  operatorScopes,
  sidebarNavId,
}: ShellControlsProps) {
  const pathname = usePathname();
  const collapsed = useShellSidebarCollapsed();
  const mobileNavigation = useIntentModule(mobileNavigationLoader);
  const [mobileOpen, setMobileOpen] = useState(false);
  const mobileTriggerRef = useRef<HTMLButtonElement>(null);
  const activeWorkspaceId = workspaceIdFromPath(pathname);
  const linkWorkspaceId = activeWorkspaceId ?? workspaces[0]?.id ?? null;
  const mobileLoading =
    mobileOpen &&
    mobileNavigation.status === INTENT_MODULE_STATUS.loading;
  const mobileFailed =
    mobileNavigation.status === INTENT_MODULE_STATUS.error;

  useEffect(() => {
    setAnalyticsContext({
      workspaceId: activeWorkspaceId,
      workspaceCount: workspaces.length,
    });
  }, [activeWorkspaceId, workspaces.length]);

  useEffect(() => {
    setMobileOpen(false);
  }, [pathname]);

  useEffect(
    () => () => {
      shellSidebarState.reset();
    },
    [],
  );

  function preloadMobileNavigation() {
    void mobileNavigationLoader.preload();
  }

  function openMobileNavigation() {
    setMobileOpen(true);
    const request =
      mobileNavigation.status === INTENT_MODULE_STATUS.error
        ? mobileNavigationLoader.retry()
        : mobileNavigationLoader.preload();
    void request;
  }

  return (
    <>
      <Button
        ref={mobileTriggerRef}
        type="button"
        aria-label={mobileFailed ? "Retry navigation" : "Open navigation"}
        aria-busy={mobileLoading}
        variant="ghost"
        size="icon"
        className="md:hidden -ml-2"
        onFocus={preloadMobileNavigation}
        onPointerEnter={() => {
          if (maySpeculateOnHover()) preloadMobileNavigation();
        }}
        onClick={openMobileNavigation}
      >
        {mobileLoading ? (
          <Spinner size="sm" srLabel="Loading navigation" />
        ) : mobileFailed ? (
          <RefreshCwIcon size={18} />
        ) : (
          <MenuIcon size={18} />
        )}
      </Button>
      {mobileNavigation.module ? (
        <mobileNavigation.module.default
          open={mobileOpen}
          pathname={pathname}
          workspaceId={linkWorkspaceId}
          operatorScopes={operatorScopes}
          onOpenChange={setMobileOpen}
          restoreFocus={() => mobileTriggerRef.current?.focus()}
        />
      ) : null}
      <Button
        type="button"
        aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
        aria-expanded={!collapsed}
        aria-controls={sidebarNavId}
        variant="ghost"
        size="icon"
        className="hidden md:inline-flex -ml-2"
        onClick={shellSidebarState.toggle}
      >
        {collapsed ? (
          <PanelLeftOpenIcon size={18} />
        ) : (
          <PanelLeftCloseIcon size={18} />
        )}
      </Button>
      <Link
        href={
          linkWorkspaceId
            ? workspacePath(linkWorkspaceId, DEFAULT_WORKSPACE_SUBPATH)
            : "/"
        }
        className="inline-flex items-center gap-2 font-mono text-sm font-medium tracking-tight text-foreground no-underline"
        aria-label="agentsfleet home"
      >
        <WakePulse
          live
          className="inline-block w-3 h-3 rounded-full bg-pulse"
          aria-hidden="true"
        />
        <span>agentsfleet</span>
      </Link>
    </>
  );
}
