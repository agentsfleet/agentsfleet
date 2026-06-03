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
} from "@usezombie/design-system";
import { createWorkspaceAction } from "@/app/(dashboard)/actions";
import { presentErrorString } from "@/lib/errors";

type Props = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
};

export default function CreateWorkspaceDialog({ open, onOpenChange }: Props) {
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
      onOpenChange(false);
      // The action already revalidated the layout (active-workspace switch);
      // refresh the client tree so the switcher reflects the new workspace.
      router.refresh();
    });
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>New workspace</DialogTitle>
          <DialogDescription>
            A workspace isolates agents and credentials; billing rolls up to
            your tenant. Leave the name blank to have one generated.
          </DialogDescription>
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
