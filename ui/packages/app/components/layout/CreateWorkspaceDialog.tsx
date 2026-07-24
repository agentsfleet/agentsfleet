"use client";

import { type FormEvent, useId } from "react";
import {
  ActionForm,
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

type Props = {
  open: boolean;
  pending: boolean;
  error: string | null;
  onOpenChange: (open: boolean) => void;
  onSubmit: (name?: string) => void | Promise<void>;
  restoreFocus?: () => void;
};

const WORKSPACE_DESCRIPTION =
  "Use workspaces to organize fleets, teammates, and credentials within your tenant. Leave the name blank to generate one.";
const WORKSPACE_NAME_FIELD = "workspace-name";
const CREATE_FORM_TEST_ID = "workspace-create-form";

export default function CreateWorkspaceDialog({
  open,
  pending,
  error,
  onOpenChange,
  onSubmit,
  restoreFocus,
}: Props) {
  const inputId = useId();

  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const value = new FormData(event.currentTarget).get(WORKSPACE_NAME_FIELD);
    const name = typeof value === "string" ? value.trim() || undefined : undefined;
    void onSubmit(name);
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        onCloseAutoFocus={(event) => {
          event.preventDefault();
          restoreFocus?.();
        }}
      >
        <ActionForm onSubmit={submit} data-testid={CREATE_FORM_TEST_ID} aria-busy={pending}>
          <DialogHeader>
            <DialogTitle>Create workspace</DialogTitle>
            <DialogDescription>{WORKSPACE_DESCRIPTION}</DialogDescription>
          </DialogHeader>
          <div className="space-y-2">
            <Label htmlFor={inputId}>Name (optional)</Label>
            <Input
              id={inputId}
              name={WORKSPACE_NAME_FIELD}
              placeholder="acme-prod"
              autoComplete="off"
              disabled={pending}
              data-testid="workspace-name-input"
            />
          </div>
          {error ? (
            <Alert variant="destructive" className="mt-4 text-xs" data-testid="workspace-create-error">
              {error}
            </Alert>
          ) : null}
          <DialogFooter>
            <Button type="button" variant="ghost" onClick={() => onOpenChange(false)}>
              {pending ? "Hide" : "Cancel"}
            </Button>
            <Button type="submit" disabled={pending} data-testid="workspace-create-submit">
              {pending ? <Spinner size="sm" srLabel="Creating" /> : null}
              Create workspace
            </Button>
          </DialogFooter>
        </ActionForm>
      </DialogContent>
    </Dialog>
  );
}
