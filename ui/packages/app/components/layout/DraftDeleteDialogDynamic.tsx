"use client";

import { useEffect } from "react";
import { createIntentModuleLoader, useIntentModule } from "@/components/domain/island-dynamic/intent-module-loader";
import { IntentDialogStatus } from "@/components/domain/island-dynamic/IntentDialogStatus";
import type { DraftDeleteDialogProps } from "./DraftDeleteDialog";

const draftDeleteDialogLoader = createIntentModuleLoader(() => import("./DraftDeleteDialog"));

export function DraftDeleteDialogDynamic(props: DraftDeleteDialogProps) {
  const dialog = useIntentModule(draftDeleteDialogLoader);
  useEffect(() => {void draftDeleteDialogLoader.preload();}, []);
  if (dialog.module) {
    const {DraftDeleteDialog} = dialog.module;
    return <DraftDeleteDialog {...props} />;
  }
  return <IntentDialogStatus open title="Delete setup chat" description="Loading chat controls…"
    errorMessage="Could not load the chat controls." status={dialog.status} onOpenChange={props.onClose}
    onRetry={() => {void draftDeleteDialogLoader.retry();}} />;
}
