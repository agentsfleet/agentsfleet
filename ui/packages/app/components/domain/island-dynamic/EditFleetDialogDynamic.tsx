"use client";

import { useEffect } from "react";
import type { PlatformEntry } from "@/lib/api/platform-library-types";
import {
  createIntentModuleLoader,
  useIntentModule,
} from "./intent-module-loader";
import { IntentDialogStatus } from "./IntentDialogStatus";

const editFleetDialogLoader = createIntentModuleLoader(
  () =>
    import(
      "@/app/(dashboard)/admin/fleet-libraries/components/EditFleetDialog"
    ),
);

export function preloadEditFleetDialog() {
  void editFleetDialogLoader.preload();
}

export default function EditFleetDialogDynamic({
  entry,
  returnFocusTo,
  open,
  onOpenChange,
  onSaved,
}: {
  entry: PlatformEntry;
  returnFocusTo?: HTMLElement;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSaved: (entry: PlatformEntry) => void;
}) {
  const dialog = useIntentModule(editFleetDialogLoader);

  useEffect(() => {
    if (open) void editFleetDialogLoader.preload();
  }, [open]);

  if (dialog.module) {
    const EditFleetDialog = dialog.module.default;
    return (
      <EditFleetDialog
        entry={entry}
        returnFocusTo={returnFocusTo}
        open={open}
        onOpenChange={onOpenChange}
        onSaved={onSaved}
      />
    );
  }

  return (
    <IntentDialogStatus
      open={open}
      title="Edit fleet library"
      description="Loading fleet library editor…"
      errorMessage="Could not load the fleet library editor."
      status={dialog.status}
      onOpenChange={onOpenChange}
      onRetry={() => void editFleetDialogLoader.retry()}
    />
  );
}
