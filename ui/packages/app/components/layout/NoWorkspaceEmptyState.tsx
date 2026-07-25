"use client";

import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import {
  Button,
  EmptyState,
  PageHeader,
  PageLayout,
  PageTitle,
} from "@agentsfleet/design-system";
import { FolderIcon } from "lucide-react";
import CreateWorkspaceDialogDynamic from "@/components/domain/island-dynamic/CreateWorkspaceDialogDynamic";
import { DEFAULT_WORKSPACE_SUBPATH, workspacePath } from "@/lib/workspace-routes";
import { useWorkspaceCreation } from "@/components/layout/WorkspaceCreationProvider";

// Zero-workspace entry state. A brand-new tenant (mid-provision, or one whose
// only workspace was deleted) reaches `/` with an empty owned list — this is a
// calm create-first surface, not a broken page.
export default function NoWorkspaceEmptyState() {
  const router = useRouter();
  const [open, setOpen] = useState(false);
  const createButtonRef = useRef<HTMLButtonElement>(null);

  const creation = useWorkspaceCreation({
    onSuccess: (workspace) => {
      setOpen(false);
      router.push(workspacePath(workspace.workspace_id, DEFAULT_WORKSPACE_SUBPATH));
    },
  });

  function setCreateDialogOpen(nextOpen: boolean) {
    if (nextOpen) {
      creation.reset();
      setOpen(true);
      return;
    }

    setOpen(false);
    creation.dismiss();
  }

  function restoreCreateFocus() {
    createButtonRef.current?.focus();
  }

  function runPrimaryAction() {
    if (creation.pending) return;
    const workspace = creation.settlingWorkspace;
    if (workspace) {
      router.push(workspacePath(workspace.workspace_id, DEFAULT_WORKSPACE_SUBPATH));
      return;
    }
    setCreateDialogOpen(true);
  }

  return (
    <PageLayout>
      <PageHeader description="Create your first workspace to install fleets, wire integrations, and store secrets.">
        <PageTitle>Welcome</PageTitle>
      </PageHeader>
      <EmptyState
        icon={<FolderIcon size={32} strokeWidth={1.75} />}
        title="No workspace yet"
        description="A workspace isolates your fleets and credentials. Create one to get started."
        action={
          <Button
            ref={createButtonRef}
            type="button"
            onClick={runPrimaryAction}
            aria-disabled={creation.pending || undefined}
            className="aria-disabled:pointer-events-none aria-disabled:cursor-wait aria-disabled:opacity-50"
            data-testid="create-first-workspace"
          >
            {creation.settlingWorkspace ? "Open workspace" : "Create workspace"}
          </Button>
        }
      />
      <CreateWorkspaceDialogDynamic
        open={open}
        pending={creation.pending}
        error={creation.error}
        onOpenChange={setCreateDialogOpen}
        onSubmit={creation.create}
        restoreFocus={restoreCreateFocus}
      />
    </PageLayout>
  );
}
