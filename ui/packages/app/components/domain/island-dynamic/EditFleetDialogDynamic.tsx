"use client";

import { useEffect } from "react";
import type { PlatformCatalogEntry } from "@/lib/types";
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
  open,
  onOpenChange,
  onSaved,
}: {
  entry: PlatformCatalogEntry;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSaved: (entry: PlatformCatalogEntry) => void;
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
