"use client";

import { createContext, useCallback, useContext, type ReactNode } from "react";
import { NO_SENDER_NAMES, isNamedTeammate, nameSender, type SenderNames } from "@/lib/events/sender-names";

/** How a person's turn is labelled: the words, and whether they are shown. */
export type SenderIdentity = { label: string; shown: boolean };
import { useSenderLabel } from "./FleetMessageRow";

// The thread's senders by name. Rows are rendered by a callback the thread
// primitive owns, so the names reach them through context, beside the fleet's
// own name in `SenderLabelProvider`.
const SenderNamesContext = createContext<SenderNames>(NO_SENDER_NAMES);

export function SenderNamesProvider({ names, children }: { names: SenderNames; children: ReactNode }) {
  return <SenderNamesContext.Provider value={names}>{children}</SenderNamesContext.Provider>;
}

/** How an actor's messages are labelled in this thread. */
export function useNameSender(): (actor: string) => SenderIdentity {
  const names = useContext(SenderNamesContext);
  const fleetName = useSenderLabel();
  return useCallback(
    (actor: string) => ({ label: nameSender(actor, fleetName, names), shown: isNamedTeammate(actor, names) }),
    [fleetName, names],
  );
}
