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
  Input,
} from "@agentsfleet/design-system";
import { createCredentialAction, deleteCredentialAction } from "../actions";
import { presentErrorString } from "@/lib/errors";
import { CREDENTIAL_NAME_MAX, parseCredentialDataObject } from "../lib/credential-data";

export type EditCredentialDialogProps = {
  workspaceId: string;
  /** The credential being edited. Its name is the reference key Fleets resolve. */
  name: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
};

// Two edit shapes. Rotate keeps the name (a same-name re-store overwrites the
// secret in place); rename is create-new-then-delete-old and breaks every
// `${secrets.<old>...}` reference, so it lives behind an Advanced disclosure.
const EDIT_MODE = { rotate: "rotate", rename: "rename" } as const;
type EditMode = (typeof EDIT_MODE)[keyof typeof EDIT_MODE];

const DATA_REQUIRED = "Re-enter the secret as a JSON object";

/**
 * Edit a stored credential. Rotate (default) overwrites the secret value under
 * the same name via the create upsert; rename (Advanced) creates the new name
 * then deletes the old, with a loud warning because it breaks references. The
 * vault never returns plaintext, so both modes re-enter the secret body.
 */
export default function EditCredentialDialog({
  workspaceId,
  name,
  open,
  onOpenChange,
}: EditCredentialDialogProps) {
  const router = useRouter();
  const [mode, setMode] = useState<EditMode>(EDIT_MODE.rotate);
  const [dataJson, setDataJson] = useState("");
  const [newName, setNewName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();

  const isRename = mode === EDIT_MODE.rename;

  function reset() {
    setMode(EDIT_MODE.rotate);
    setDataJson("");
    setNewName("");
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
    const parsed = parseCredentialDataObject(dataJson, DATA_REQUIRED);
    if (!parsed.ok) {
      setError(parsed.message);
      return;
    }
    const target = isRename ? newName.trim() : name;
    if (isRename && (target === "" || target.length > CREDENTIAL_NAME_MAX)) {
      setError(`New name must be 1–${CREDENTIAL_NAME_MAX} characters`);
      return;
    }
    if (isRename && target === name) {
      setError("New name matches the current name — use Rotate to replace the value");
      return;
    }

    startTransition(async () => {
      const created = await createCredentialAction(workspaceId, { name: target, data: parsed.data });
      if (!created.ok) {
        setError(
          presentErrorString({
            errorCode: created.errorCode,
            message: created.error,
            action: isRename ? "rename the credential" : "rotate the credential",
          }),
        );
        return;
      }
      // Rename only: drop the old name AFTER the new one is safely stored, so a
      // failure here never strands the tenant with neither name.
      if (isRename) {
        const removed = await deleteCredentialAction(workspaceId, name);
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
          <DialogTitle>Edit credential &ldquo;{name}&rdquo;</DialogTitle>
          <DialogDescription>
            Values are write-only — re-enter the full secret to replace what&apos;s stored.
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

          {isRename ? (
            <div className="space-y-2">
              <Label htmlFor="edit-new-name">New name</Label>
              <Input
                id="edit-new-name"
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                placeholder={name}
                spellCheck={false}
                autoComplete="off"
              />
              <Alert variant="warning" className="text-xs">
                Renaming breaks Fleets that reference{" "}
                <code>{`\${secrets.${name}...}`}</code> — they fail to resolve until you update them.
                The old name is removed once the new one is stored.
              </Alert>
            </div>
          ) : (
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={() => setMode(EDIT_MODE.rename)}
            >
              Advanced — rename
            </Button>
          )}

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
            {isRename ? "Rename" : "Rotate"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
