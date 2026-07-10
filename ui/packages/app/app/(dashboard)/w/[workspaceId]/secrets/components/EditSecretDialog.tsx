"use client";

import { useState, useTransition } from "react";
import { useRouter } from "next/navigation";
import {
  Alert,
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Label,
  Spinner,
  Textarea,
} from "@agentsfleet/design-system";
import { createSecretAction } from "../actions";
import { presentErrorString } from "@/lib/errors";
import { SECRET_DATA_REENTER_REQUIRED, parseSecretDataObject } from "../lib/secret-data";

export type EditSecretDialogProps = {
  workspaceId: string;
  /** The secret being rotated. Its name is the reference key fleets resolve. */
  name: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
};

/**
 * Rotate a stored secret: re-store its value under the same name (the create
 * upsert overwrites in place). The vault never returns plaintext, so the value
 * is always re-entered. This dialog has one job — renaming lives in its own
 * RenameSecretDialog, reached from the Name column.
 */
export default function EditSecretDialog({
  workspaceId,
  name,
  open,
  onOpenChange,
}: EditSecretDialogProps) {
  const router = useRouter();
  const [dataJson, setDataJson] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();

  function reset() {
    setDataJson("");
    setError(null);
  }

  function handleOpenChange(next: boolean) {
    // Block dismiss mid-save. The dialog is parent-controlled and only ever
    // emits a close, so reset unconditionally before propagating.
    if (pending) return;
    reset();
    onOpenChange(next);
  }

  function onSubmit() {
    setError(null);
    const parsed = parseSecretDataObject(dataJson, SECRET_DATA_REENTER_REQUIRED);
    if (!parsed.ok) {
      setError(parsed.message);
      return;
    }

    startTransition(async () => {
      const created = await createSecretAction(workspaceId, { name, data: parsed.data });
      if (!created.ok) {
        setError(
          presentErrorString({
            errorCode: created.errorCode,
            message: created.error,
            action: "rotate the secret",
          }),
        );
        return;
      }
      reset();
      onOpenChange(false);
      router.refresh();
    });
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>Edit secret &ldquo;{name}&rdquo;</DialogTitle>
          <DialogDescription>
            Saved values are hidden. Paste the full replacement value to update this secret.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="edit-data">Data (JSON object)</Label>
            <Textarea
              id="edit-data"
              rows={6}
              spellCheck={false}
              autoComplete="off"
              placeholder='{"api_key": "sk-..."}'
              className="font-mono text-sm"
              value={dataJson}
              onChange={(e) => setDataJson(e.target.value)}
            />
          </div>

          {error ? (
            <Alert variant="destructive" className="text-xs">
              {error}
            </Alert>
          ) : null}
        </div>

        <DialogFooter className="flex-col gap-2 sm:flex-row sm:gap-2">
          <Button type="button" variant="ghost" disabled={pending} onClick={() => handleOpenChange(false)}>
            Cancel
          </Button>
          <Button type="button" disabled={pending} onClick={onSubmit} aria-busy={pending ? "true" : undefined}>
            {pending ? <Spinner size="sm" srLabel="Saving" /> : null}
            Rotate
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
