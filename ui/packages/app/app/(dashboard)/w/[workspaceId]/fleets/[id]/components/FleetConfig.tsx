"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { Trash2Icon } from "lucide-react";
import { Button, ConfirmDialog } from "@agentsfleet/design-system";
import { deleteFleetAction } from "../../actions";
import { workspacePath } from "@/lib/workspace-routes";
import { presentErrorString } from "@/lib/errors";
import { DELETE_MEMORY_TRAP_NOTICE } from "./console-copy";

type Props = {
  workspaceId: string;
  fleetId: string;
  fleetName: string;
};

export default function FleetConfig({
  workspaceId,
  fleetId,
  fleetName,
}: Props) {
  const router = useRouter();
  const [open, setOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function onConfirm() {
    setError(null);
    const result = await deleteFleetAction(workspaceId, fleetId);
    if (!result.ok) {
      throw new Error(
        presentErrorString({
          errorCode: result.errorCode,
          message: result.error,
          action: "delete this fleet",
        }),
      );
    }
    // No router.refresh() — calling refresh immediately after push races
    // the URL commit (the same surface the install flow hits); the fleets list
    // is `force-dynamic` so it re-fetches on its own.
    router.push(workspacePath(workspaceId, "fleets"));
  }

  return (
    <div className="rounded-md border border-border bg-card p-4">
      <Button
        type="button"
        onClick={() => setOpen(true)}
        variant="destructive"
        size="sm"
      >
        <Trash2Icon size={14} /> Delete fleet
      </Button>

      <ConfirmDialog
        open={open}
        onOpenChange={setOpen}
        title={`Delete ${fleetName}?`}
        description={`This permanently deletes the fleet. ${DELETE_MEMORY_TRAP_NOTICE}`}
        confirmLabel="Yes, delete"
        intent="destructive"
        onConfirm={onConfirm}
        errorMessage={error}
        // onConfirm wraps every failure in `throw new Error(presentErrorString(...))`,
        // so onError always receives an Error.
        onError={(e) => setError((e as Error).message)}
      />
    </div>
  );
}
