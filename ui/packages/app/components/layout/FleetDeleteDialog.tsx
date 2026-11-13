"use client";

import { useEffect, useRef, useState } from "react";
import { ConfirmDialog } from "@agentsfleet/design-system";
import { FleetSchema } from "@/lib/api/draft-types";
import type { NativeFleetSummary } from "@/lib/api/fleet-detail";
import { setupRequest, setupErrorMessage, SetupUnavailable } from "@/lib/api/draft-request";
import { deleteFleetAction } from "@/app/(dashboard)/w/[workspaceId]/fleets/actions";

export function FleetDeleteDialog({fleet, workspace, onRemoved, onClose}: {
  fleet: NativeFleetSummary; workspace: string; onRemoved: (id: string) => void; onClose: () => void;
}) {
  const owner = useRef<AbortController | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => () => owner.current?.abort(), []);
  const remove = async (): Promise<void> => {
    if (owner.current) return;
    const request = new AbortController(); owner.current = request; setError(null);
    try {
      const path = `/live/v1/workspaces/${encodeURIComponent(workspace)}/fleets/${encodeURIComponent(fleet.id)}`;
      // The list row carries no version, so read the current tag. DELETE works
      // in any status and ends live work itself; nothing archives first.
      const current = await setupRequest({path}, FleetSchema, request.signal);
      request.signal.throwIfAborted();
      const deleted = await deleteFleetAction(workspace, fleet.id, current.etag);
      request.signal.throwIfAborted();
      if (!deleted.ok) throw new SetupUnavailable(deleted.error, deleted.status);
      onRemoved(fleet.id);
    } catch (cause) {
      if (!request.signal.aborted) setError(setupErrorMessage(cause));
      throw cause;
    } finally {owner.current = null;}
  };
  return <ConfirmDialog open onOpenChange={onClose} title="Delete fleet?" intent="destructive"
    description={`Permanently delete “${fleet.name}”? This ends any live work and removes its chat and activity. Published library versions stay available.`}
    confirmLabel="Delete fleet" onConfirm={remove} errorMessage={error} />;
}
