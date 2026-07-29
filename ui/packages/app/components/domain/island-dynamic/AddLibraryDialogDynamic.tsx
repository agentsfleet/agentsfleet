"use client";

import { useEffect, useState } from "react";
import { PlusIcon } from "lucide-react";
import { TooltipButton } from "@agentsfleet/design-system";
import {
  createIntentModuleLoader,
  INTENT_MODULE_STATUS,
  maySpeculateOnHover,
  useIntentModule,
} from "./intent-module-loader";
import { IntentDialogStatus } from "./IntentDialogStatus";

const CREATE_FLEET_LIBRARY_TOOLTIP =
  "Create a fleet library entry from GitHub.";
const addLibraryDialogLoader = createIntentModuleLoader(
  () =>
    import(
      "@/app/(dashboard)/w/[workspaceId]/fleets/new/AddLibraryDialog"
    ),
);

export function preloadAddLibraryDialog() {
  void addLibraryDialogLoader.preload();
}

export default function AddLibraryDialogDynamic({
  workspaceId,
  triggerLabel = "Create fleet library",
  defaultOpen = false,
}: {
  workspaceId: string;
  triggerLabel?: string;
  defaultOpen?: boolean;
}) {
  const dialog = useIntentModule(addLibraryDialogLoader);
  const [activated, setActivated] = useState(defaultOpen);

  useEffect(() => {
    if (activated) void addLibraryDialogLoader.preload();
  }, [activated]);

  function openDialog() {
    setActivated(true);
    const request =
      dialog.status === INTENT_MODULE_STATUS.error
        ? addLibraryDialogLoader.retry()
        : addLibraryDialogLoader.preload();
    void request;
  }

  if (activated && dialog.module) {
    const AddLibraryDialog = dialog.module.default;
    return (
      <AddLibraryDialog
        workspaceId={workspaceId}
        triggerLabel={triggerLabel}
        defaultOpen
      />
    );
  }

  if (activated) {
    return (
      <IntentDialogStatus
        open
        title={triggerLabel}
        description="Loading the library entry form…"
        errorMessage="The library entry form could not be loaded."
        status={dialog.status}
        onOpenChange={setActivated}
        onRetry={() => void addLibraryDialogLoader.retry()}
      />
    );
  }

  const failed = dialog.status === INTENT_MODULE_STATUS.error;
  return (
    <TooltipButton
      type="button"
      size="sm"
      tooltip={CREATE_FLEET_LIBRARY_TOOLTIP}
      aria-label={failed ? `Retry ${triggerLabel}` : undefined}
      onFocus={preloadAddLibraryDialog}
      onPointerEnter={() => {
        if (maySpeculateOnHover()) preloadAddLibraryDialog();
      }}
      onClick={openDialog}
    >
      <PlusIcon size={14} />
      {failed ? `Retry ${triggerLabel}` : triggerLabel}
    </TooltipButton>
  );
}
