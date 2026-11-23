"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { TurnRow } from "@/lib/api/turns";
import { fleetTurnDetailUrl } from "@/lib/api/turns-types";
import { readTurnDetail, stopTurn, TURN_STOP_KIND } from "@/lib/api/turn-stop";
import { readTurnDetailRoute } from "@/lib/streaming/fleet-stream-detail-reader";
import type { FleetTurn } from "@/lib/streaming/fleet-stream-row";
import { stoppableTurn } from "./stoppable-turn";

export type TurnStop = {
  /** Whether a turn of this fleet runs, so Stop shows. */
  visible: boolean;
  /** A Stop for the current turn is in flight. */
  stopping: boolean;
  stop: () => void;
};

/**
 * The composer's Stop over the fleet's live turns. The answer is applied to
 * the thread as the saved row it is, so the turn reads "Stopped" even when no
 * frame announces the PATCH. A 409 means the answer landed first: the saved
 * row is read again and shown, with no alarm, because the answer stands.
 */
export function useTurnStop(
  workspaceId: string,
  fleetId: string,
  turns: readonly FleetTurn[],
  reconcileRows: (rows: TurnRow[]) => void,
): TurnStop {
  const turnId = useMemo(() => stoppableTurn(turns), [turns]);
  const [stoppingId, setStoppingId] = useState<string | null>(null);
  const owner = useRef<AbortController | null>(null);
  useEffect(() => () => owner.current?.abort(), []);
  const stop = useCallback(() => {
    if (turnId === null || stoppingId === turnId) return;
    owner.current?.abort();
    const controller = new AbortController();
    owner.current = controller;
    setStoppingId(turnId);
    void (async () => {
      const result = await stopTurn(fleetTurnDetailUrl(workspaceId, fleetId, turnId), controller.signal, readTurnDetail);
      if (result.kind === TURN_STOP_KIND.STOPPED) reconcileRows([result.answer]);
      if (result.kind === TURN_STOP_KIND.FINISHED) {
        const saved = await readTurnDetailRoute(workspaceId, fleetId, turnId);
        if (saved.ok) reconcileRows([saved.data]);
      }
      if (owner.current === controller) setStoppingId(null);
    })();
  }, [workspaceId, fleetId, turnId, stoppingId, reconcileRows]);
  const visible = turnId !== null;
  const stopping = visible && stoppingId === turnId;
  // One object per change, so the memoised viewport does not redraw on every frame.
  return useMemo(() => ({ visible, stopping, stop }), [visible, stopping, stop]);
}
