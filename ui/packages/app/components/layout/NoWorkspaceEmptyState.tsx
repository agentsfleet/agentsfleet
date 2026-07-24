"use client";

import { useState } from "react";
import { Button, EmptyState, PageHeader, PageLayout, PageTitle } from "@agentsfleet/design-system";
import { LayoutDashboardIcon } from "lucide-react";
import CreateWorkspaceDialogDynamic from "@/components/domain/island-dynamic/CreateWorkspaceDialogDynamic";

// Zero-workspace entry state. A brand-new tenant (mid-provision, or one whose
// only workspace was deleted) reaches `/` with an empty owned list — this is a
// calm create-first surface, not a broken page. On create the dialog itself
// navigates to `/w/<newId>/fleets` (see CreateWorkspaceDialog), so no id is threaded
// back here.
export default function NoWorkspaceEmptyState() {
  const [open, setOpen] = useState(false);
  return (
    <PageLayout>
      <PageHeader description="Create your first workspace to install fleets, wire integrations, and store secrets.">
        <PageTitle>Welcome</PageTitle>
      </PageHeader>
      <EmptyState
        icon={<LayoutDashboardIcon size={32} />}
        title="No workspace yet"
        description="A workspace isolates your fleets and credentials. Create one to get started."
        action={
          <Button type="button" onClick={() => setOpen(true)} data-testid="create-first-workspace">
            Create workspace
          </Button>
        }
      />
      <CreateWorkspaceDialogDynamic open={open} onOpenChange={setOpen} />
    </PageLayout>
  );
}
