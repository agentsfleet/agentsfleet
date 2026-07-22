"use client";

import nextDynamic from "next/dynamic";
import { Skeleton } from "@agentsfleet/design-system";
import type { FleetThreadProps } from "./FleetThread";

// Client-Component shim around `next/dynamic` so the parent Server
// Component (`app/(dashboard)/w/[workspaceId]/fleets/[id]/page.tsx`) can opt the chat
// surface out of SSR without itself becoming client-side. Next.js 16
// forbids `ssr: false` directly in Server Components.
//
// Code-split: `@assistant-ui/react` ships in a separate chunk that
// only loads after this client component mounts. The skeleton takes the
// same share of the frame the thread will, so the swap costs no layout
// shift on a console that claims the viewport.

const InnerFleetThread = nextDynamic(
  () =>
    import("./FleetThread").then((mod) => ({ default: mod.FleetThread })),
  {
    ssr: false,
    loading: () => <Skeleton className="min-h-0 w-full flex-1 rounded-md" />,
  },
);

export default function FleetThreadDynamic(props: FleetThreadProps) {
  return <InnerFleetThread {...props} />;
}
