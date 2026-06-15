"use client";

import nextDynamic from "next/dynamic";
import { Skeleton } from "@agentsfleet/design-system";
import type { AgentThreadProps } from "./AgentThread";

// Client-Component shim around `next/dynamic` so the parent Server
// Component (`app/(dashboard)/agents/[id]/page.tsx`) can opt the chat
// surface out of SSR without itself becoming client-side. Next.js 16
// forbids `ssr: false` directly in Server Components.
//
// Code-split: `@assistant-ui/react` ships in a separate chunk that
// only loads after this client component mounts. The skeleton fills
// the panel's grid cell during the swap to avoid a CLS bump — `h-96`
// (24rem stock Tailwind) is the documented scale; no arbitrary needed.

const InnerAgentThread = nextDynamic(
  () =>
    import("./AgentThread").then((mod) => ({ default: mod.AgentThread })),
  {
    ssr: false,
    loading: () => <Skeleton className="h-96 w-full rounded-md" />,
  },
);

export default function AgentThreadDynamic(props: AgentThreadProps) {
  return <InnerAgentThread {...props} />;
}
