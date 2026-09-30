"use client";

import nextDynamic from "next/dynamic";
import type { ComponentProps } from "react";
import { Skeleton } from "@agentsfleet/design-system";

// Client shim for the invite dialog (react-hook-form + zod). It owns its own
// trigger button, so the loading fallback reserves the `size="sm"` Button's
// footprint (h-8) to avoid a layout shift while the chunk loads after
// hydration. Keeps the dialog body out of the /settings/members initial bundle.
const InnerInviteDialog = nextDynamic(
  () =>
    import("@/app/(dashboard)/settings/members/components/InviteDialog").then((mod) => ({
      default: mod.default,
    })),
  { ssr: false, loading: () => <Skeleton className="h-8 w-24 rounded-md" /> },
);

export default function InviteDialogDynamic(props: ComponentProps<typeof InnerInviteDialog>) {
  return <InnerInviteDialog {...props} />;
}
