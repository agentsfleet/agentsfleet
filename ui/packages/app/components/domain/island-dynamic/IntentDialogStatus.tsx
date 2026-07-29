"use client";

import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Spinner,
} from "@agentsfleet/design-system";
import {
  INTENT_MODULE_STATUS,
  type IntentModuleSnapshot,
} from "./intent-module-loader";

type IntentDialogStatusProps = {
  description: string;
  errorMessage: string;
  onOpenChange: (open: boolean) => void;
  onRetry: () => void;
  open: boolean;
  status: IntentModuleSnapshot<unknown>["status"];
  title: string;
};

export function IntentDialogStatus({
  description,
  errorMessage,
  onOpenChange,
  onRetry,
  open,
  status,
  title,
}: IntentDialogStatusProps) {
  const failed = status === INTENT_MODULE_STATUS.error;
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent aria-busy={!failed}>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription role={failed ? "alert" : undefined}>
            {failed ? errorMessage : "This should only take a moment."}
          </DialogDescription>
        </DialogHeader>
        {failed ? (
          <DialogFooter>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={() => onOpenChange(false)}
            >
              Close
            </Button>
            <Button
              type="button"
              variant="secondary"
              size="sm"
              onClick={onRetry}
            >
              Retry
            </Button>
          </DialogFooter>
        ) : (
          <div className="flex min-h-24 items-center justify-center">
            <Spinner size="md" label={description} />
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}
