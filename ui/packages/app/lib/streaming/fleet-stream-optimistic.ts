import { outcomeForStatus } from "@/lib/events/event-summary";
import { AGENTSFLEET_EVENT_STATUS, type FleetEvent } from "./fleet-stream-row";

// The optimistic-row reducers: what the operator's own message looks like
// before the server names it, and how it is folded onto the server's row when
// the identifier arrives. Pure, like the frame reducers; the registry owns the
// entry they run against.

/** The operator's message as the thread shows it before the server answers. */
export function optimisticRow(tempId: string, text: string, actor: string): FleetEvent {
  return {
    id: tempId,
    role: "user",
    actor,
    text,
    // The operator's own message is the trigger; the fleet has not replied
    // yet, so the reply is empty and the outcome floor is set for shape.
    reply: "",
    outcome: outcomeForStatus(AGENTSFLEET_EVENT_STATUS.RECEIVED),
    failureLabel: null,
    failureDetail: null,
    createdAt: new Date(),
    status: AGENTSFLEET_EVENT_STATUS.OPTIMISTIC,
  };
}

export type Reconciled = {
  events: FleetEvent[];
  /** True when the server's row had already finished by the time the
   * identifier arrived — the reply is on it and nothing more is coming. */
  alreadyComplete: boolean;
};

/**
 * Fold the optimistic row onto the server's. When the server row already
 * landed (a live `event_received` beat the 202), the operator's text is
 * grafted onto it — the live frame carries no body, so the optimistic row is
 * the only holder of what was said — and the temp row leaves. Otherwise the
 * temp row simply takes the real identifier and the server's opening status.
 */
export function reconcileRows(
  prev: FleetEvent[],
  tempId: string,
  realEventId: string,
): Reconciled {
  const serverEvent = prev.find((event) => event.id === realEventId);
  if (serverEvent) {
    const temp = prev.find((event) => event.id === tempId);
    const grafted =
      temp !== undefined && serverEvent.text.length === 0
        ? prev.map((event) => (event === serverEvent ? { ...event, text: temp.text } : event))
        : prev;
    return {
      events: grafted.filter((event) => event.id !== tempId),
      alreadyComplete: serverEvent.status !== AGENTSFLEET_EVENT_STATUS.RECEIVED,
    };
  }
  return {
    events: prev.map((event) =>
      event.id === tempId
        ? { ...event, id: realEventId, status: AGENTSFLEET_EVENT_STATUS.RECEIVED }
        : event,
    ),
    alreadyComplete: false,
  };
}
