"use client";

import { useEffect, useState, useTransition } from "react";
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
} from "@agentsfleet/design-system";
import { createWorkspaceAction } from "@/app/(dashboard)/actions";
import type { CreateWorkspaceResponse } from "@/lib/api/workspaces";
import { DEFAULT_WORKSPACE_SUBPATH, workspacePath } from "@/lib/workspace-routes";
import { presentErrorString } from "@/lib/errors";

type Props = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onCreated?: (workspace: CreateWorkspaceResponse) => void;
};

const WORKSPACE_DESCRIPTION =
  "Use workspaces to organize fleets, teammates, and credentials within your tenant. Leave the name blank to generate one.";

export default function CreateWorkspaceDialog({ open, onOpenChange, onCreated }: Props) {
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();
  const router = useRouter();

  // Reset the form when the dialog closes. The component stays mounted while
  // closed (only the dialog content unmounts), so without this a typed-but-
  // cancelled name — or a stale error — would persist into the next open. The
  // cleanup fires on the open→closed transition and on unmount, covering every
  // dismiss path (Cancel, Escape, overlay click) uniformly.
  useEffect(() => {
    if (!open) return;
    return () => {
      setName("");
      setError(null);
    };
  }, [open]);

  function submit() {
    if (pending) return;
    setError(null);
    startTransition(async () => {
      // Blank name → omit so the server picks a Heroku-style name.
      const result = await createWorkspaceAction({ name: name.trim() || undefined });
      if (!result.ok) {
        setError(
          presentErrorString({
            errorCode: result.errorCode,
            message: result.error,
            action: "create workspace",
          }),
        );
        return;
      }
      setName("");
      onCreated?.(result.data);
      onOpenChange(false);
      // Selection is the URL: navigate straight to the new workspace's home so
      // the switcher, nav, and pages all key off it — no cookie, no refresh.
      router.push(workspacePath(result.data.workspace_id, DEFAULT_WORKSPACE_SUBPATH));
    });
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Create workspace</DialogTitle>
          <DialogDescription>{WORKSPACE_DESCRIPTION}</DialogDescription>
        </DialogHeader>
        <div className="space-y-2">
          <Label htmlFor="workspace-name">Name (optional)</Label>
          <Input
            id="workspace-name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") submit();
            }}
            placeholder="acme-prod"
            autoComplete="off"
            data-testid="workspace-name-input"
          />
        </div>
        {error ? (
          <Alert variant="destructive" className="text-xs" data-testid="workspace-create-error">
            {error}
          </Alert>
        ) : null}
        <DialogFooter>
          <Button
            type="button"
            variant="ghost"
            onClick={() => onOpenChange(false)}
            disabled={pending}
          >
            Cancel
          </Button>
          <Button
            type="button"
            onClick={submit}
            disabled={pending}
            data-testid="workspace-create-submit"
          >
            {pending ? <Spinner size="sm" srLabel="Creating" /> : null}
            Create workspace
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
