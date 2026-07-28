"use client";

import { useEffect, useState } from "react";
import { PlusIcon } from "lucide-react";
import { Spinner, TooltipButton } from "@agentsfleet/design-system";
import {
  createIntentModuleLoader,
  INTENT_MODULE_STATUS,
  maySpeculateOnHover,
  useIntentModule,
} from "./intent-module-loader";

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

  const failed = dialog.status === INTENT_MODULE_STATUS.error;
  const loading =
    activated && dialog.status === INTENT_MODULE_STATUS.loading;
  return (
    <TooltipButton
      type="button"
      size="sm"
      tooltip={CREATE_FLEET_LIBRARY_TOOLTIP}
      aria-label={failed ? `Retry ${triggerLabel}` : undefined}
      aria-busy={loading}
      onFocus={preloadAddLibraryDialog}
      onPointerEnter={() => {
        if (maySpeculateOnHover()) preloadAddLibraryDialog();
      }}
      onClick={openDialog}
    >
      {loading ? (
        <Spinner size="sm" srLabel={`Loading ${triggerLabel}`} />
      ) : (
        <PlusIcon size={14} />
      )}
      {failed ? `Retry ${triggerLabel}` : triggerLabel}
    </TooltipButton>
  );
}
