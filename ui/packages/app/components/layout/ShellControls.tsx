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
import { cn, Button, Spinner, BrandMark } from "@agentsfleet/design-system";
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
  SIDEBAR_COLUMN,
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

function useMobileNavigation(workspaces: TenantWorkspace[]) {
  const pathname = usePathname();
  const mobileNavigation = useIntentModule(mobileNavigationLoader);
  const [mobileOpen, setMobileOpen] = useState(false);
  const mobileTriggerRef = useRef<HTMLButtonElement>(null);
  const activeWorkspaceId = workspaceIdFromPath(pathname);
  const linkWorkspaceId = activeWorkspaceId ?? workspaces[0]?.id ?? null;

  useEffect(() => {
    setAnalyticsContext({
      workspaceId: activeWorkspaceId,
      workspaceCount: workspaces.length,
    });
  }, [activeWorkspaceId, workspaces.length]);

  useEffect(() => {
    setMobileOpen(false);
  }, [pathname]);

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

  return { pathname, mobileNavigation, mobileOpen, setMobileOpen, mobileTriggerRef, linkWorkspaceId, preloadMobileNavigation, openMobileNavigation };
}

export function ShellControls({ workspaces, operatorScopes, sidebarNavId }: ShellControlsProps) {
  const navigation = useMobileNavigation(workspaces);
  const { mobileNavigation, mobileOpen, pathname, linkWorkspaceId, setMobileOpen, mobileTriggerRef } = navigation;
  const collapsed = useShellSidebarCollapsed();
  return (
    <>
      {mobileNavigation.module ? (
        <mobileNavigation.module.default
          workspaces={workspaces}
          open={mobileOpen}
          pathname={pathname}
          workspaceId={linkWorkspaceId}
          operatorScopes={operatorScopes}
          onOpenChange={setMobileOpen}
          restoreFocus={() => mobileTriggerRef.current?.focus()}
        />
      ) : null}
      <div
        data-testid="shell-leading-cluster"
        className={cn(
          "flex shrink-0 items-center gap-2 px-4 md:gap-4 md:px-5",
          "transition-all duration-snap ease-snap",
          collapsed
            ? `${SIDEBAR_COLUMN.header.collapsed} md:justify-center`
            : `${SIDEBAR_COLUMN.header.expanded} md:justify-between`,
        )}
      >
        <MobileNavigationTrigger navigation={navigation} />
        <BrandLink workspaceId={linkWorkspaceId} collapsed={collapsed} />
        <SidebarToggle sidebarNavId={sidebarNavId} collapsed={collapsed} />
      </div>
    </>
  );
}

function MobileNavigationTrigger({ navigation }: { navigation: ReturnType<typeof useMobileNavigation> }) {
  const { mobileNavigation, mobileOpen, mobileTriggerRef, preloadMobileNavigation, openMobileNavigation } = navigation;
  const mobileLoading = mobileOpen && mobileNavigation.status === INTENT_MODULE_STATUS.loading;
  const mobileFailed = mobileNavigation.status === INTENT_MODULE_STATUS.error;
  return (
    <Button
      ref={mobileTriggerRef}
      type="button"
      aria-label={mobileFailed ? "Retry navigation" : "Open navigation"}
      aria-busy={mobileLoading}
      variant="ghost"
      size="icon"
      className="size-11 shrink-0 md:hidden -ml-2"
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
  );
}

function SidebarToggle({
  sidebarNavId,
  collapsed,
}: Pick<ShellControlsProps, "sidebarNavId"> & { collapsed: boolean }) {
  useEffect(() => () => shellSidebarState.reset(), []);
  return (
    <Button
      type="button"
      aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
      aria-expanded={!collapsed}
      aria-controls={sidebarNavId}
      variant="ghost"
      size="icon"
      className="hidden shrink-0 md:inline-flex"
      onClick={shellSidebarState.toggle}
    >
      {collapsed ? (
        <PanelLeftOpenIcon size={18} />
      ) : (
        <PanelLeftCloseIcon size={18} />
      )}
    </Button>
  );
}

function BrandLink({
  workspaceId,
  collapsed,
}: { workspaceId: string | null; collapsed: boolean }) {
  return (
    <Link
      href={
        workspaceId
          ? workspacePath(workspaceId, DEFAULT_WORKSPACE_SUBPATH)
          : "/"
      }
      className={cn(
        "inline-flex shrink-0 items-center gap-2.5 font-sans text-body-sm font-medium tracking-tight text-foreground no-underline",
        collapsed && "md:hidden",
      )}
      aria-label="agentsfleet home"
    >
      <BrandMark className="size-4.5" />
      <span>agentsfleet</span>
    </Link>
  );
}
