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
  Input,
  Label,
  Spinner,
  Textarea,
} from "@agentsfleet/design-system";
import { createSecretAction, deleteSecretAction } from "../actions";
import { presentErrorString } from "@/lib/errors";
import {
  SECRET_DATA_REENTER_REQUIRED,
  SECRET_NAME_MAX,
  parseSecretDataObject,
} from "../lib/secret-data";

export type RenameSecretDialogProps = {
  workspaceId: string;
  /** The secret being renamed. Its current name is the reference key fleets resolve. */
  name: string;
  /**
   * Every existing secret name in the workspace. Renaming to one of these would
   * upsert (overwrite) that secret's value before the old name is deleted —
   * collapsing two secrets into one and destroying the collided secret. Guarded
   * against here so the rename never silently clobbers another secret.
   */
  existingNames: readonly string[];
  open: boolean;
  onOpenChange: (open: boolean) => void;
};

// Renaming is create-new-then-delete-old: there is no in-place rename endpoint,
// and the vault never returns plaintext, so the new name must be stored with a
// freshly re-entered value before the old name is dropped. The warning is
// generic by design — the platform never indexes which fleets reference a given
// secret name, so it can't name them.
const RENAME_WARNING =
  "Any fleets using this secret still point to the old name. They may fail after the rename until you update them.";
const NAME_LENGTH_INVALID = `New name must be 1–${SECRET_NAME_MAX} characters`;
const NAME_UNCHANGED = "New name matches the current name — use Edit to replace the value instead";
const nameTakenMessage = (n: string) =>
  `A secret named "${n}" already exists — delete it or pick a different name`;

/**
 * Rename a stored secret. There is no in-place rename, so this creates the new
 * name (with a re-entered value, since the vault never returns plaintext) then
 * deletes the old — in that order, so a failure never strands the tenant with
 * neither name. Lives in its own dialog, reached from the Name column; Edit
 * (the pencil) is rotate-only.
 */
export default function RenameSecretDialog({
  workspaceId,
  name,
  existingNames,
  open,
  onOpenChange,
}: RenameSecretDialogProps) {
  const router = useRouter();
  const [newName, setNewName] = useState("");
  const [dataJson, setDataJson] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();

  function reset() {
    setNewName("");
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
    const target = newName.trim();
    if (target === "" || target.length > SECRET_NAME_MAX) {
      setError(NAME_LENGTH_INVALID);
      return;
    }
    if (target === name) {
      setError(NAME_UNCHANGED);
      return;
    }
    // Renaming to another existing name would upsert (overwrite) that secret
    // then delete this one — silent data loss. Reject before touching the API.
    if (existingNames.includes(target)) {
      setError(nameTakenMessage(target));
      return;
    }

    startTransition(async () => {
      const created = await createSecretAction(workspaceId, { name: target, data: parsed.data });
      if (!created.ok) {
        setError(
          presentErrorString({
            errorCode: created.errorCode,
            message: created.error,
            action: "rename the secret",
          }),
        );
        return;
      }
      // Drop the old name only AFTER the new one is safely stored, so a failure
      // here never strands the tenant with neither name.
      const removed = await deleteSecretAction(workspaceId, name);
      if (!removed.ok) {
        // The new name IS stored — refresh so the list shows it (and the
        // still-present old name), keep the dialog open with a clear message
        // so the user can delete the old name from the list manually.
        router.refresh();
        setError(
          presentErrorString({
            errorCode: removed.errorCode,
            message: removed.error,
            action: `remove the old name "${name}" — "${target}" was created; delete "${name}" from the list`,
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
          <DialogTitle>Rename secret &ldquo;{name}&rdquo;</DialogTitle>
          <DialogDescription>
            Choose a new name and paste the value again. Saved values are hidden, so we create a
            new secret and remove the old one after it saves.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="rename-new-name">New name</Label>
            <Input
              id="rename-new-name"
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              placeholder={name}
              spellCheck={false}
              autoComplete="off"
            />
          </div>

          <div className="space-y-2">
            <Label htmlFor="rename-data">Data (JSON object)</Label>
            <Textarea
              id="rename-data"
              rows={6}
              spellCheck={false}
              autoComplete="off"
              placeholder='{"api_key": "sk-..."}'
              className="font-mono text-sm"
              value={dataJson}
              onChange={(e) => setDataJson(e.target.value)}
            />
          </div>

          <Alert variant="warning" className="text-xs">
            {RENAME_WARNING}
          </Alert>

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
            Rename
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
