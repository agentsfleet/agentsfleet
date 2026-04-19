"use client";

import { useCallback, useId, useState, type ReactNode } from "react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "./Dialog";
import { Button } from "./Button";

/*
 * ConfirmDialog — accessible confirmation overlay. Client boundary:
 * useState + useCallback gate the in-flight action. Composed from the
 * shared Dialog + Button primitives so the destructive intent lights up
 * the same `variant="destructive"` Button surface used across the
 * product.
 */
export interface ConfirmDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description?: ReactNode;
  confirmLabel?: string;
  cancelLabel?: string;
  /** Colouring + semantics. `destructive` surfaces a red confirm button. */
  intent?: "default" | "destructive";
  /** Async action. The dialog disables both buttons while it resolves. */
  onConfirm: () => void | Promise<void>;
  /**
   * Caller-controlled. Render a custom error message above the footer.
   * Typically set from caller state in response to `onError` below.
   */
  errorMessage?: string | null;
  /**
   * Invoked when `onConfirm()` throws or rejects. Receives the thrown
   * value. Recommended: set local state from this that feeds
   * `errorMessage`. When omitted, rejections are swallowed silently so
   * they do not surface as unhandled promise rejections (React drops
   * async event-handler rejections in production).
   */
  onError?: (error: unknown) => void;
}

export function ConfirmDialog({
  open,
  onOpenChange,
  title,
  description,
  confirmLabel = "Confirm",
  cancelLabel = "Cancel",
  intent = "default",
  onConfirm,
  errorMessage,
  onError,
}: ConfirmDialogProps) {
  const [pending, setPending] = useState(false);
  const descId = useId();

  const handleConfirm = useCallback(async () => {
    if (pending) return;
    setPending(true);
    try {
      await onConfirm();
    } catch (err) {
      if (onError) onError(err);
    } finally {
      setPending(false);
    }
  }, [pending, onConfirm, onError]);

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!pending) onOpenChange(next);
      }}
    >
      <DialogContent
        data-slot="confirm-dialog"
        data-testid="confirm-dialog"
        role="alertdialog"
        aria-describedby={description ? descId : undefined}
        className="max-w-md"
      >
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          {description ? (
            <DialogDescription id={descId}>{description}</DialogDescription>
          ) : null}
        </DialogHeader>
        {errorMessage ? (
          <p
            role="alert"
            className="rounded border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive"
          >
            {errorMessage}
          </p>
        ) : null}
        <DialogFooter className="flex-col gap-2 sm:flex-row sm:gap-2">
          <Button
            type="button"
            variant="ghost"
            disabled={pending}
            onClick={() => onOpenChange(false)}
          >
            {cancelLabel}
          </Button>
          <Button
            type="button"
            variant={intent === "destructive" ? "destructive" : "default"}
            disabled={pending}
            onClick={handleConfirm}
            aria-busy={pending ? "true" : undefined}
          >
            {pending ? "Working…" : confirmLabel}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

export default ConfirmDialog;
