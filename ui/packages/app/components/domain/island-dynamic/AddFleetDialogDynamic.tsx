"use client";

import { useEffect } from "react";
import {
  createIntentModuleLoader,
  useIntentModule,
} from "./intent-module-loader";
import { IntentDialogStatus } from "./IntentDialogStatus";

const addFleetDialogLoader = createIntentModuleLoader(
  () =>
    import(
      "@/app/(dashboard)/admin/fleet-libraries/components/AddFleetDialog"
    ),
);

export function preloadAddFleetDialog() {
  void addFleetDialogLoader.preload();
}

export default function AddFleetDialogDynamic({
  open,
  onOpenChange,
  prefillRepo,
  prefillRef,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  prefillRepo?: string;
  prefillRef?: string;
}) {
  const dialog = useIntentModule(addFleetDialogLoader);

  useEffect(() => {
    if (open) void addFleetDialogLoader.preload();
  }, [open]);

  if (dialog.module) {
    const AddFleetDialog = dialog.module.default;
    return (
      <AddFleetDialog
        open={open}
        onOpenChange={onOpenChange}
        prefillRepo={prefillRepo}
        prefillRef={prefillRef}
      />
    );
  }

  if (!open) return null;

  return (
    <IntentDialogStatus
      open={open}
      title="Create fleet library"
      description="Loading fleet library form…"
      errorMessage="Could not load the fleet library form."
      status={dialog.status}
      onOpenChange={onOpenChange}
      onRetry={() => void addFleetDialogLoader.retry()}
    />
  );
}
