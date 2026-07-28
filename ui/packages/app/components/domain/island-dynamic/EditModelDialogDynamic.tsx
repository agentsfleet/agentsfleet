"use client";

import { useEffect } from "react";
import type { AdminModel } from "@/lib/api/admin_model_library";
import {
  createIntentModuleLoader,
  useIntentModule,
} from "./intent-module-loader";
import { IntentDialogStatus } from "./IntentDialogStatus";

const editModelDialogLoader = createIntentModuleLoader(
  () => import("@/app/(dashboard)/admin/models/components/EditModelDialog"),
);

export function preloadEditModelDialog() {
  void editModelDialogLoader.preload();
}

export default function EditModelDialogDynamic({
  model,
  onOpenChange,
  onUpdated,
}: {
  model: AdminModel;
  onOpenChange: (open: boolean) => void;
  onUpdated: (model: AdminModel) => void;
}) {
  const dialog = useIntentModule(editModelDialogLoader);

  useEffect(() => {
    void editModelDialogLoader.preload();
  }, []);

  if (dialog.module) {
    const EditModelDialog = dialog.module.default;
    return (
      <EditModelDialog
        model={model}
        onOpenChange={onOpenChange}
        onUpdated={onUpdated}
      />
    );
  }

  return (
    <IntentDialogStatus
      open
      title="Edit model"
      description="Loading model editor…"
      errorMessage="Could not load the model editor."
      status={dialog.status}
      onOpenChange={onOpenChange}
      onRetry={() => void editModelDialogLoader.retry()}
    />
  );
}
