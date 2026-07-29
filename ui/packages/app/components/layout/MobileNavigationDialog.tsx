"use client";

import {
  Dialog,
  DialogContent,
  DialogTitle,
} from "@agentsfleet/design-system";
import { SidebarNavigation } from "./SidebarNavigation";

type MobileNavigationDialogProps = {
  open: boolean;
  pathname: string;
  workspaceId: string | null;
  operatorScopes: string[];
  onOpenChange: (open: boolean) => void;
  restoreFocus: () => void;
};

export default function MobileNavigationDialog({
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
        className="sm:max-w-xs"
        onCloseAutoFocus={(event) => {
          event.preventDefault();
          restoreFocus();
        }}
      >
        <DialogTitle className="sr-only">Navigation</DialogTitle>
        <SidebarNavigation
          pathname={pathname}
          workspaceId={workspaceId}
          operatorScopes={operatorScopes}
          collapsed={false}
          gettingStartedPolling="mounted"
          onNavigate={() => onOpenChange(false)}
        />
      </DialogContent>
    </Dialog>
  );
}
