"use client";

import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import { FleetStreams } from "@/lib/streaming/fleet-stream-registry";

const StreamContext = createContext<FleetStreams | null>(null);

/** The caller keys this owner by account, workspace and signed-in subject. */
export function FleetStreamProvider({children, streams: supplied}: {children: ReactNode; streams?: FleetStreams}) {
  const [streams] = useState(() => supplied ?? new FleetStreams());
  useEffect(() => () => streams.close(), [streams]);
  return <StreamContext value={streams}>{children}</StreamContext>;
}

export function useFleetStreams(): FleetStreams {
  const streams = useContext(StreamContext);
  if (!streams) throw new Error("Fleet chat requires an identity-owned stream provider.");
  return streams;
}
