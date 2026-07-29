"use client";

import { useState } from "react";
import { PlusIcon } from "lucide-react";
import { Spinner, TooltipButton } from "@agentsfleet/design-system";
import type { AdminModel } from "@/lib/api/admin_model_library";
import {
  createIntentModuleLoader,
  INTENT_MODULE_STATUS,
  maySpeculateOnHover,
  useIntentModule,
} from "./intent-module-loader";

const CREATE_MODEL_LIBRARY_TOOLTIP =
  "Create a priced model users can choose.";
const addModelDialogLoader = createIntentModuleLoader(
  () => import("@/app/(dashboard)/admin/models/components/AddModelDialog"),
);

export function preloadAddModelDialog() {
  void addModelDialogLoader.preload();
}

export default function AddModelDialogDynamic({
  onCreated,
}: {
  onCreated: (model: AdminModel) => void;
}) {
  const dialog = useIntentModule(addModelDialogLoader);
  const [activated, setActivated] = useState(false);

  function openDialog() {
    setActivated(true);
    const request =
      dialog.status === INTENT_MODULE_STATUS.error
        ? addModelDialogLoader.retry()
        : addModelDialogLoader.preload();
    void request;
  }

  if (activated && dialog.module) {
    const AddModelDialog = dialog.module.default;
    return <AddModelDialog defaultOpen onCreated={onCreated} />;
  }

  const failed = dialog.status === INTENT_MODULE_STATUS.error;
  const loading =
    activated && dialog.status === INTENT_MODULE_STATUS.loading;
  return (
    <TooltipButton
      type="button"
      size="sm"
      tooltip={CREATE_MODEL_LIBRARY_TOOLTIP}
      aria-label={failed ? "Retry create model library" : undefined}
      aria-busy={loading}
      onFocus={preloadAddModelDialog}
      onPointerEnter={() => {
        if (maySpeculateOnHover()) preloadAddModelDialog();
      }}
      onClick={openDialog}
    >
      {loading ? (
        <Spinner size="sm" srLabel="Loading model library form" />
      ) : (
        <PlusIcon size={14} />
      )}
      {failed ? "Retry create model library" : "Create model library"}
    </TooltipButton>
  );
}
