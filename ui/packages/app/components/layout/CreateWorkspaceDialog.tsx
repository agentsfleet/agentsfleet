"use client";

import { type FormEvent, useId, useRef } from "react";
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
import {
  hasWorkspaceNameContent,
  isWorkspaceNameSafe,
  trimWorkspaceName,
  WORKSPACE_NAME_MAX_CODEPOINTS,
} from "@/lib/api/workspaces";

type Props = {
  open: boolean;
  pending: boolean;
  error: string | null;
  onOpenChange: (open: boolean) => void;
  onSubmit: (name: string) => void | Promise<void>;
  restoreFocus?: () => void;
};

const WORKSPACE_DESCRIPTION =
  "Use workspaces to organize fleets, teammates, and credentials within your organization.";
const WORKSPACE_NAME_FIELD = "workspace-name";
const WORKSPACE_NAME_REQUIRED = "Enter a workspace name.";
const WORKSPACE_NAME_TOO_LONG = `Use ${WORKSPACE_NAME_MAX_CODEPOINTS} characters or fewer.`;
const WORKSPACE_NAME_UNSAFE =
  "Remove control or directional formatting characters.";
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
  const openRef = useRef(open);
  openRef.current = open;

  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const input = event.currentTarget.elements.namedItem(
      WORKSPACE_NAME_FIELD,
    ) as HTMLInputElement;
    const name = trimWorkspaceName(input.value);
    if (!hasWorkspaceNameContent(name)) {
      input.setCustomValidity(WORKSPACE_NAME_REQUIRED);
      input.reportValidity();
      return;
    }
    if ([...name].length > WORKSPACE_NAME_MAX_CODEPOINTS) {
      input.setCustomValidity(WORKSPACE_NAME_TOO_LONG);
      input.reportValidity();
      return;
    }
    if (!isWorkspaceNameSafe(name)) {
      input.setCustomValidity(WORKSPACE_NAME_UNSAFE);
      input.reportValidity();
      return;
    }
    input.setCustomValidity("");
    void onSubmit(name);
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        onCloseAutoFocus={(event) => {
          event.preventDefault();
          if (!openRef.current) restoreFocus?.();
        }}
      >
        <ActionForm
          onSubmit={submit}
          data-testid={CREATE_FORM_TEST_ID}
          aria-busy={pending}
        >
          <DialogHeader>
            <DialogTitle>Create workspace</DialogTitle>
            <DialogDescription>{WORKSPACE_DESCRIPTION}</DialogDescription>
          </DialogHeader>
          <div className="space-y-2">
            <Label htmlFor={inputId}>Name</Label>
            <Input
              id={inputId}
              name={WORKSPACE_NAME_FIELD}
              placeholder="acme-prod"
              autoComplete="off"
              disabled={pending}
              required
              aria-required="true"
              onInput={(event) => event.currentTarget.setCustomValidity("")}
              data-testid="workspace-name-input"
            />
          </div>
          {error ? (
            <Alert
              variant="destructive"
              className="mt-4 text-xs"
              data-testid="workspace-create-error"
            >
              {error}
            </Alert>
          ) : null}
          <DialogFooter>
            <Button
              type="button"
              variant="ghost"
              onClick={() => onOpenChange(false)}
            >
              {pending ? "Hide" : "Cancel"}
            </Button>
            <Button
              type="submit"
              disabled={pending}
              data-testid="workspace-create-submit"
            >
              {pending ? <Spinner size="sm" srLabel="Creating" /> : null}
              Create
            </Button>
          </DialogFooter>
        </ActionForm>
      </DialogContent>
    </Dialog>
  );
}
