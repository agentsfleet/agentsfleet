"use client";

import type { Fleet, Schedule } from "@/lib/api/draft-types";
import { useState } from "react";
import { useRouter } from "next/navigation";
import { Alert } from "@agentsfleet/design-system";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets-types";
import { ScheduleSchema } from "@/lib/api/draft-types";
import { scheduleSyncPath } from "@/lib/api/draft-schedules";
import { REQUEST_METHOD, setupErrorMessage, setupRequest } from "@/lib/api/draft-request";
import { SetupProgress } from "@/components/domain/chat/SetupProgress";
import { ScheduleSync } from "@/components/domain/chat/ScheduleSync";
import { useMounted } from "@/hooks/use-mounted";
import { setFleetStatusAction } from "../../actions";

export function FleetRecoveryContent({workspace, fleet, setupPending}: {workspace: string; fleet: Fleet; setupPending: boolean}) {
  const router = useRouter();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Server-rendered controls must wait until their client handlers can respond.
  const unavailable = !useMounted() || busy;
  const resume = async (): Promise<void> => {
    setBusy(true); setError(null);
    try {
      const result = await setFleetStatusAction(workspace, fleet.id, AGENTSFLEET_STATUS.ACTIVE, fleet.etag);
      if (!result.ok && result.status !== 412) setError(result.error);
      router.refresh();
    } catch (cause) { setError(cause instanceof Error ? cause.message : "Setup could not be confirmed. Refresh to read its current state."); }
    finally { setBusy(false); }
  };
  const retrySchedule = async (schedule: Schedule): Promise<void> => {
    setBusy(true); setError(null);
    try {
      const base = `/live/v1/workspaces/${encodeURIComponent(workspace)}`;
      await setupRequest({method: REQUEST_METHOD.post, path: scheduleSyncPath(base, schedule)}, ScheduleSchema, new AbortController().signal);
      router.refresh();
    } catch (cause) { setError(setupErrorMessage(cause)); }
    finally { setBusy(false); }
  };
  return <div className="mb-lg min-h-0 overflow-y-auto space-y-md">
    {error ? <Alert variant="destructive">{error}</Alert> : null}
    {setupPending ? <SetupProgress fleet={fleet} workspace={workspace} busy={unavailable} onContinue={() => { void resume(); }}
      onRetrySchedule={(schedule) => { void retrySchedule(schedule); }} />
      : <ScheduleSync schedules={fleet.schedules} busy={unavailable} onRetry={(schedule) => { void retrySchedule(schedule); }} />}
  </div>;
}
