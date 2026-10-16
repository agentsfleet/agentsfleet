"use client";

import {
  Dialog,
  DialogContent,
  DialogTitle,
} from "@agentsfleet/design-system";
import { SidebarNavigation } from "./SidebarNavigation";

import type { TenantWorkspace } from "@/lib/api/workspaces";

type MobileNavigationDialogProps = {
  workspaces: TenantWorkspace[];
  open: boolean;
  pathname: string;
  workspaceId: string | null;
  operatorScopes: string[];
  onOpenChange: (open: boolean) => void;
  restoreFocus: () => void;
};

export default function MobileNavigationDialog({
  workspaces,
  open,
  pathname,
  workspaceId,
  operatorScopes,
  onOpenChange,
  restoreFocus,
}: MobileNavigationDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="h-dvh max-h-dvh rounded-none sm:max-w-xs"
        onCloseAutoFocus={(event) => {
          event.preventDefault();
          restoreFocus();
        }}
      >
        <DialogTitle className="sr-only">Navigation</DialogTitle>
        <SidebarNavigation
          workspaces={workspaces}
          pathname={pathname}
          workspaceId={workspaceId}
          operatorScopes={operatorScopes}
          collapsed={false}
          onNavigate={() => onOpenChange(false)}
        />
      </DialogContent>
    </Dialog>
  );
}
