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
} from "@usezombie/design-system";
import { createCredentialAction, deleteCredentialAction } from "../actions";
import { presentErrorString } from "@/lib/errors";

export type EditCredentialDialogProps = {
  workspaceId: string;
  /** The credential being edited. Its name is the reference key agents resolve. */
  name: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
};

// Two edit shapes. Rotate keeps the name (a same-name re-store overwrites the
// secret in place); rename is create-new-then-delete-old and breaks every
// `${secrets.<old>...}` reference, so it lives behind an Advanced disclosure.
const EDIT_MODE = { rotate: "rotate", rename: "rename" } as const;
type EditMode = (typeof EDIT_MODE)[keyof typeof EDIT_MODE];

const NAME_MAX = 64;
const DATA_REQUIRED = "Re-enter the secret as a JSON object";
const DATA_NOT_OBJECT = "Data must be a JSON object — strings, arrays, and scalars are rejected";
const DATA_EMPTY = "Object must have at least one field";

type ParsedData = { ok: true; data: Record<string, unknown> } | { ok: false; message: string };

// Values are write-only at the vault, so editing re-enters the full secret
// rather than pre-filling it. Parse + shape-check before we ever call the API.
function parseDataObject(raw: string): ParsedData {
  const trimmed = raw.trim();
  if (trimmed === "") return { ok: false, message: DATA_REQUIRED };
  let parsed: unknown;
  try {
    parsed = JSON.parse(trimmed);
  } catch (err) {
    return { ok: false, message: `Invalid JSON: ${err instanceof Error ? err.message : "parse error"}` };
  }
  if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed)) {
    return { ok: false, message: DATA_NOT_OBJECT };
  }
  if (Object.keys(parsed).length === 0) return { ok: false, message: DATA_EMPTY };
  return { ok: true, data: parsed as Record<string, unknown> };
}

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
    if (pending) return;
    if (!next) reset();
    onOpenChange(next);
  }

  function onSubmit() {
    setError(null);
    const parsed = parseDataObject(dataJson);
    if (!parsed.ok) {
      setError(parsed.message);
      return;
    }
    const target = isRename ? newName.trim() : name;
    if (isRename && (target === "" || target.length > NAME_MAX)) {
      setError(`New name must be 1–${NAME_MAX} characters`);
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
          setError(
            presentErrorString({
              errorCode: removed.errorCode,
              message: removed.error,
              action: "remove the old credential name",
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
                Renaming breaks agents that reference{" "}
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
              aria-expanded={false}
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
