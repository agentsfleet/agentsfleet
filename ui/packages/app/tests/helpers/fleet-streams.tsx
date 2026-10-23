import type { ReactNode } from "react";
import { FleetStreams } from "@/lib/streaming/fleet-stream-registry";
import { FleetStreamProvider } from "@/components/domain/FleetStreamProvider";
export * from "@/lib/streaming/fleet-stream-registry";

export const streams = new FleetStreams();
export const subscribe = streams.subscribe.bind(streams);
export const getSnapshot = streams.getSnapshot.bind(streams);
export const retryConnection = streams.retryConnection.bind(streams);
export const reconcileServerRows = streams.reconcileServerRows.bind(streams);
export const reconcileServerFacts = streams.reconcileServerFacts.bind(streams);
export const appendOptimistic = streams.appendOptimistic.bind(streams);
export const reconcileOptimistic = streams.reconcileOptimistic.bind(streams);
export const discardOptimistic = streams.discardOptimistic.bind(streams);
export const __resetRegistryForTests = (): void => streams.close();
export function streamWrapper({children}: {children: ReactNode}) {
  return <FleetStreamProvider streams={streams}>{children}</FleetStreamProvider>;
}
