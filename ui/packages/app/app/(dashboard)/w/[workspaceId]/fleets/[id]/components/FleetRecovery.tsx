"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { Alert } from "@agentsfleet/design-system";
import type { Fleet } from "@/lib/api/draft-types";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets-types";
import { START_KIND } from "@/lib/api/draft-types";
import { SetupProgress } from "@/components/domain/chat/SetupProgress";
import { setFleetStatusAction } from "../../actions";

export function FleetRecovery({workspace, fleet}: {workspace: string; fleet: Fleet}) {
  const router = useRouter();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const resume = async (): Promise<void> => {
    setBusy(true); setError(null);
    try {
      const result = await setFleetStatusAction(workspace, fleet.id, AGENTSFLEET_STATUS.ACTIVE, fleet.etag);
      if (!result.ok && result.status !== 412) setError(result.error);
      router.refresh();
    } catch (cause) { setError(cause instanceof Error ? cause.message : "Setup could not be confirmed. Refresh to read its current state."); }
    finally { setBusy(false); }
  };
  if (fleet.status !== AGENTSFLEET_STATUS.INSTALLING && !(fleet.status === AGENTSFLEET_STATUS.ACTIVE &&
    fleet.start.kind === START_KIND.runNow && fleet.start.state !== "accepted")) return null;
  return <div className="mb-lg space-y-md">
    {error ? <Alert variant="destructive">{error}</Alert> : null}
    <SetupProgress fleet={fleet} workspace={workspace} busy={busy} onContinue={() => { void resume(); }} />
  </div>;
}
