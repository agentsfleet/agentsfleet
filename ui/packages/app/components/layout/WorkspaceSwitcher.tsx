"use client";

import { useState } from "react";
import { usePathname } from "next/navigation";
import type { TenantWorkspace } from "@/lib/api/workspaces";
import { workspaceIdFromPath } from "@/lib/workspace-routes";
import {
  createIntentModuleLoader,
  INTENT_MODULE_STATUS,
  maySpeculateOnHover,
  useIntentModule,
} from "@/components/domain/island-dynamic/intent-module-loader";
import { WorkspaceSwitcherTrigger } from "./WorkspaceSwitcherTrigger";

type Props = {
  workspaces: TenantWorkspace[];
};

const workspaceSwitcherLoader = createIntentModuleLoader(
  () => import("./WorkspaceSwitcherMenu"),
);

export default function WorkspaceSwitcher({ workspaces }: Props) {
  const pathname = usePathname();
  const workspaceSwitcher = useIntentModule(workspaceSwitcherLoader);
  const [activated, setActivated] = useState(false);
  const [open, setOpen] = useState(false);
  const routedId = workspaceIdFromPath(pathname);
  const active = routedId
    ? workspaces.find((workspace) => workspace.id === routedId)
    : workspaces[0];
  const activeLabel =
    routedId && !active
      ? "Current workspace"
      : (active?.name ?? (active ? "Unnamed workspace" : "No workspace"));

  function preloadWorkspaceMenu() {
    void workspaceSwitcherLoader.preload();
  }

  function openWorkspaceMenu() {
    setActivated(true);
    setOpen(true);
    const request =
      workspaceSwitcher.status === INTENT_MODULE_STATUS.error
        ? workspaceSwitcherLoader.retry()
        : workspaceSwitcherLoader.preload();
    void request;
  }

  if (activated && workspaceSwitcher.module) {
    const Menu = workspaceSwitcher.module.default;
    return (
      <Menu
        open={open}
        workspaces={workspaces}
        onOpenChange={setOpen}
      />
    );
  }

  const failed = workspaceSwitcher.status === INTENT_MODULE_STATUS.error;
  const loading =
    activated && workspaceSwitcher.status === INTENT_MODULE_STATUS.loading;
  return (
    <div className="inline-flex flex-wrap items-center gap-2">
      <WorkspaceSwitcherTrigger
        activeLabel={activeLabel}
        busy={loading}
        failed={failed}
        aria-label={failed ? "Retry workspace menu" : "Select workspace"}
        aria-busy={loading}
        data-testid="workspace-switcher"
        onFocus={preloadWorkspaceMenu}
        onPointerEnter={() => {
          if (maySpeculateOnHover()) preloadWorkspaceMenu();
        }}
        onClick={openWorkspaceMenu}
      />
    </div>
  );
}
