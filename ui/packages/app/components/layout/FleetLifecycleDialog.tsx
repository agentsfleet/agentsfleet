"use client";

import { useEffect, useRef, useState } from "react";
import { ConfirmDialog } from "@agentsfleet/design-system";
import { useCurrentUser } from "@/lib/auth/client";
import { advanceFleetLifecycle, type FleetLifecycleAction, type FleetLifecycleCheckpoint } from "@/lib/fleet-lifecycle";
import { useFleetNotice } from "./FleetNoticeProvider";

type Props = {
  workspace: string; fleet: {id: string; name: string}; action: FleetLifecycleAction;
  onRemoved: (id: string) => void; onClose: () => void;
};

export function FleetLifecycleDialog(props: Props) {
  const {userId} = useCurrentUser();
  return <LifecycleConfirmation key={`${userId}:${props.workspace}:${props.fleet.id}:${props.action}`} {...props} />;
}

function LifecycleConfirmation({workspace, fleet, action, onRemoved, onClose}: Props) {
  const owner = useRef<AbortController | null>(null);
  const [checkpoint, setCheckpoint] = useState<FleetLifecycleCheckpoint>({stage: "read"});
  const [error, setError] = useState<string | null>(null);
  const showNotice = useFleetNotice();
  useEffect(() => () => owner.current?.abort(), []);
  const confirm = async (): Promise<void> => {
    if (owner.current) return;
    const current = new AbortController();
    owner.current = current;
    setError(null);
    try {
      const result = await advanceFleetLifecycle({workspace, fleetId: fleet.id, action, signal: current.signal}, checkpoint);
      if (current.signal.aborted || result.kind === "cancelled") return;
      if (result.kind === "retry") {setCheckpoint(result.checkpoint); setError(result.message); return;}
      showNotice({outcome: result.outcome, historyHref: `/w/${workspace}/fleets/${fleet.id}`});
      onRemoved(fleet.id);
    } finally {owner.current = null;}
  };
  const label = checkpoint.stage.startsWith("check_") ? "Check status" : checkpoint.stage === "delete" ? "Retry deletion"
    : error ? "Try again" : action === "archive" ? "Archive" : "Permanently delete";
  return <ConfirmDialog open onOpenChange={onClose} intent="destructive" title={action === "archive" ? "Archive fleet?" : "Permanently delete fleet?"}
    description={action === "archive" ? `Permanently stop “${fleet.name}” and retain its chat and activity. It cannot be restored.`
      : `Permanently stop and delete “${fleet.name}”, including its chat, activity and fleet memory. Published plugin versions remain available.`}
    confirmLabel={label} onConfirm={confirm} errorMessage={error} />;
}
