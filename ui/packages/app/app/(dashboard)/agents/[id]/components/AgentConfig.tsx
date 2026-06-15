"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { Trash2Icon } from "lucide-react";
import { Button, ConfirmDialog } from "@agentsfleet/design-system";
import { deleteAgentAction } from "../../actions";
import { presentErrorString } from "@/lib/errors";

type Props = {
  workspaceId: string;
  agentId: string;
  agentName: string;
};

export default function AgentConfig({
  workspaceId,
  agentId,
  agentName,
}: Props) {
  const router = useRouter();
  const [open, setOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function onConfirm() {
    setError(null);
    const result = await deleteAgentAction(workspaceId, agentId);
    if (!result.ok) {
      throw new Error(
        presentErrorString({
          errorCode: result.errorCode,
          message: result.error,
          action: "delete this agent",
        }),
      );
    }
    // No router.refresh() — calling refresh immediately after push races
    // the URL commit (same surface InstallAgentForm hit); /agents is
    // `force-dynamic` so it re-fetches on its own.
    router.push("/agents");
  }

  return (
    <div className="rounded-md border border-border bg-card p-4">
      <p className="mb-4 text-sm text-muted-foreground">
        Rename, pause, and resume become available once the backend adds{" "}
        <code className="font-mono text-xs">PATCH</code> /{" "}
        <code className="font-mono text-xs">:pause</code> /{" "}
        <code className="font-mono text-xs">:resume</code> endpoints. Delete
        works today.
      </p>

      <Button
        type="button"
        onClick={() => setOpen(true)}
        variant="destructive"
        size="sm"
      >
        <Trash2Icon size={14} /> Delete agent
      </Button>

      <ConfirmDialog
        open={open}
        onOpenChange={setOpen}
        title={`Delete ${agentName}?`}
        description="This removes the agent. In-flight runs should be stopped first."
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
