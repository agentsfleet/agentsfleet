import { outcomeForStatus } from "@/lib/turns/turn-summary";
import { AGENTSFLEET_TURN_STATUS, FOLDED_OUTCOME, type FleetTurn } from "./fleet-stream-row";

// The optimistic-row reducers: what the operator's own message looks like
// before the server names it, and how it is folded onto the server's row when
// the identifier arrives. Pure, like the frame reducers; the registry owns the
// entry they run against.

/** The operator's message as the thread shows it before the server answers. */
export function optimisticRow(tempId: string, text: string, actor: string, sentAs?: string): FleetTurn {
  return {
    ...(sentAs === undefined ? {} : { sentAs }),
    id: tempId,
    role: "user",
    actor,
    text,
    // The operator's own message is the trigger; the fleet has not replied
    // yet, so the reply is empty and the outcome floor is set for shape.
    reply: "",
    outcome: outcomeForStatus(AGENTSFLEET_TURN_STATUS.RECEIVED),
    failureLabel: null,
    failureDetail: null,
    createdAt: new Date(),
    status: AGENTSFLEET_TURN_STATUS.OPTIMISTIC,
    clientTimestamp: true,
    submittedAtMs: performance.now(),
  };
}

export type Reconciled = {
  turns: FleetTurn[];
  /** True when the server's row had already finished by the time the
   * identifier arrived — the reply is on it and nothing more is coming. */
  alreadyComplete: boolean;
  /** Whether the thread already held the server's row. */
  loaded: boolean;
};

/**
 * Fold the optimistic row onto the server's. When the server row already
 * landed (a live `turn_received` beat the 202), the operator's text is
 * grafted onto it — the live frame carries no body, so the optimistic row is
 * the only holder of what was said — and the temp row leaves. Otherwise the
 * temp row simply takes the real identifier and the server's opening status.
 */
export function reconcileRows(
  prev: FleetTurn[],
  tempId: string,
  realTurnId: string,
): Reconciled {
  const serverTurn = prev.find((turn) => turn.id === realTurnId);
  if (serverTurn) {
    const temp = prev.find((turn) => turn.id === tempId);
    const grafted = temp === undefined ? prev : prev.map((turn) => turn === serverTurn
      ? { ...turn, text: serverTurn.text.length === 0 ? temp.text : serverTurn.text, submittedAtMs: temp.submittedAtMs }
      : turn);
    return {
      turns: grafted.filter((turn) => turn.id !== tempId),
      alreadyComplete: serverTurn.status !== AGENTSFLEET_TURN_STATUS.RECEIVED,
      loaded: true,
    };
  }
  return {
    turns: prev.map((turn) =>
      turn.id === tempId
        ? { ...turn, id: realTurnId, status: AGENTSFLEET_TURN_STATUS.RECEIVED }
        : turn,
    ),
    alreadyComplete: false,
    loaded: false,
  };
}

/** A send the live run took: named by the server, settled with no reply of its
 * own, and labelled so the operator reads where it went. */
export function foldRow(prev: FleetTurn[], tempId: string, realTurnId: string): FleetTurn[] {
  return prev.map((turn) =>
    turn.id === tempId
      ? { ...turn, id: realTurnId, status: AGENTSFLEET_TURN_STATUS.FOLDED, outcome: FOLDED_OUTCOME }
      : turn,
  );
}
