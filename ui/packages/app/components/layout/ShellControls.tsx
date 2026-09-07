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
  return (
    <>
      <MobileNavigationTrigger navigation={navigation} />
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
      <BrandLink workspaceId={linkWorkspaceId} />
      <SidebarToggle sidebarNavId={sidebarNavId} />
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

function SidebarToggle({ sidebarNavId }: Pick<ShellControlsProps, "sidebarNavId">) {
  const collapsed = useShellSidebarCollapsed();
  useEffect(() => () => shellSidebarState.reset(), []);
  return (
    <Button
      type="button"
      aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
      aria-expanded={!collapsed}
      aria-controls={sidebarNavId}
      variant="ghost"
      size="icon"
      className="hidden md:inline-flex"
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

function BrandLink({ workspaceId }: { workspaceId: string | null }) {
  return (
    <Link
      href={
        workspaceId
          ? workspacePath(workspaceId, DEFAULT_WORKSPACE_SUBPATH)
          : "/"
      }
      className="inline-flex shrink-0 items-center gap-2 font-sans text-sm font-medium tracking-tight text-foreground no-underline"
      aria-label="agentsfleet home"
    >
      <WakePulse
        live
        className="inline-block w-3 h-3 rounded-full bg-pulse"
        aria-hidden="true"
      />
      <span>agentsfleet</span>
    </Link>
  );
}
