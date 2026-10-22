"use client";

import { useEffect, useRef, useState } from "react";
import { ConfirmDialog } from "@agentsfleet/design-system";
import { FleetSchema, SETUP_HEADERS } from "@/lib/api/draft-types";
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
      let current = await setupRequest({path}, FleetSchema, request.signal);
      if (current.status !== "killed") current = await setupRequest({path, method: "PATCH", body: {status: "killed"},
        headers: {[SETUP_HEADERS.match]: current.etag}}, FleetSchema, request.signal);
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
    description={`Stop and permanently delete “${fleet.name}”? This removes its chat and activity. Published library versions stay available.`}
    confirmLabel={fleet.status === "killed" ? "Delete fleet" : "Stop and delete"} onConfirm={remove} errorMessage={error} />;
}
