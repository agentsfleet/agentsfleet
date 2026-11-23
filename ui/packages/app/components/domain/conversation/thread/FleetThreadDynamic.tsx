"use client";

import nextDynamic from "next/dynamic";
import { RouteLoading } from "@/components/layout/RouteLoading";

// Client-Component shim around `next/dynamic` so the parent Server
// Component (`app/(dashboard)/w/[workspaceId]/fleets/[id]/page.tsx`) can opt the chat
// surface out of SSR without itself becoming client-side. The fallback fills
// the same frame while the separate assistant-ui chunk loads.

const FleetThreadDynamic = nextDynamic(() => import("./FleetThread").then(module => module.FleetThread), {
  ssr: false,
  loading: () => <RouteLoading label="Loading fleet chat" />,
});

export default FleetThreadDynamic;
